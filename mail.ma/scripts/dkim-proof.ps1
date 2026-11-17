#requires -Version 5.1
<#
    Tira Mail - DKIM end-to-end proof.

    What this proves, and why it is not a unit test
    ----------------------------------------------
    Stalwart claims the domain has active DKIM keys and the zone file shows a
    `p=` record. That is configuration. This script proves the bytes:

      1. a message submitted over authenticated SMTP is signed on the way out;
      2. the raw message is fetched back byte-for-byte over IMAP;
      3. the body hash in `bh=` matches the relaxed-canonicalized body;
      4. both `b=` signatures (RSA-2048 and Ed25519) verify against the keys
         Stalwart itself stores, so no public DNS is needed;
      5. the selectors in the signatures match the active keys for `d=`.

    Canonicalization is implemented here on purpose. Calling a library that
    accepts anything would hide the exact failure mode we care about: a message
    that looks signed but whose relaxed body hash is wrong (trailing spaces,
    folded headers, trailing empty lines).

    Cryptography
    ------------
    RSA-2048 is verified with .NET's PKCS#1 v1.5 SHA-256 implementation.
    Windows PowerShell 5.1 has no Ed25519 provider, so this script embeds a
    small RFC 8032 verifier (BigInteger field arithmetic + SHA-512) and runs
    two RFC 8032 test vectors plus a tamper check before trusting its verdict.
    RFC 8463 signs the 32-byte SHA-256 digest of the canonicalized input with
    PureEdDSA, so that digest is what the verifier is handed.

    One RFC 6376 detail is easy to get wrong and is asserted explicitly:
    Section 3.7 hash step 2 canonicalizes the DKIM-Signature field itself
    *without a trailing CRLF*, unlike the fields listed in `h=`. An off-by-one
    CRLF there makes every RSA/Ed25519 signature fail while `bh=` still matches.

    Usage
    -----
        .\scripts\dkim-proof.ps1 `
            -SenderUser youssef@uit.ac.ma -SenderPassword '...' `
            -RecipientUser aicha@tira.test -RecipientPassword '...' `
            -SkipCertificateCheck

    Passwords are mandatory (never hardcoded). -SkipCertificateCheck opts in to
    the self-signed test certificate and is refused for a non-loopback -HostName;
    without it the system validates the chain and hostname (fail closed).

    Exit code 0 only if every RSA signature verifies and every body hash matches.
#>
[CmdletBinding()]
param(
    [string] $HostName = "127.0.0.1",
    [int]    $SubmissionPort = 2526,
    [int]    $ImapPort = 1143,
    [string] $ServerName = "mail.tira.test",
    [string] $StalwartCli = "C:\tools\stalwart\stalwart-cli.exe",
    [Parameter(Mandatory)] [string] $SenderUser,
    [Parameter(Mandatory)] [string] $SenderPassword,
    [Parameter(Mandatory)] [string] $RecipientUser,
    [Parameter(Mandatory)] [string] $RecipientPassword,
    [int]    $DeliveryTimeoutSeconds = 30,
    [switch] $SkipCertificateCheck,
    [switch] $KeepRawMessage
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "proof-common.ps1")

# The TLS validation callback comes from proof-common.ps1 (New-ProofSslStream):
# system validation by default, an explicit loopback-only opt-in otherwise.

# ------------------------------------------------------------------- helpers ---

function New-TextConnection {
    param([System.IO.Stream] $Stream)
    $reader = New-Object System.IO.StreamReader($Stream, [System.Text.Encoding]::ASCII, $false, 1)
    $writer = New-Object System.IO.StreamWriter($Stream, [System.Text.Encoding]::ASCII, 1)
    $writer.NewLine = "`r`n"
    $writer.AutoFlush = $true
    [pscustomobject]@{ Reader = $reader; Writer = $writer }
}

function Read-SmtpReply {
    param([Parameter(Mandatory)] [System.IO.StreamReader] $Reader)
    $lines = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $line = $Reader.ReadLine()
        if ($null -eq $line) { throw "SMTP connection closed unexpectedly." }
        $lines.Add($line)
        if ($line.Length -ge 4 -and $line[3] -eq ' ') { return ,$lines }
        if ($line.Length -eq 3) { return ,$lines }
    }
}

function Assert-Code {
    param([string[]] $Reply, [string] $Code, [string] $Label)
    if (-not $Reply[0].StartsWith($Code)) {
        throw "ASSERT FAILED [$Label]: expected $Code, got: $($Reply -join ' | ')"
    }
    Write-Host ("  ok  {0} -> {1}" -f $Label, $Reply[0]) -ForegroundColor Green
}

# Byte-level IMAP reader. StreamReader would buffer and make literal-sized
# reads impossible; DKIM verification needs the message bytes exactly.
function Read-StreamLine {
    param([Parameter(Mandatory)] [System.IO.Stream] $Stream)
    $bytes = New-Object System.Collections.Generic.List[byte]
    while ($true) {
        $value = $Stream.ReadByte()
        if ($value -lt 0) {
            if ($bytes.Count -eq 0) { return $null }
            break
        }
        if ($value -eq 13) {
            $next = $Stream.ReadByte()
            if ($next -eq 10) { break }
            $bytes.Add([byte]$value)
            if ($next -ge 0) { $bytes.Add([byte]$next) }
            continue
        }
        $bytes.Add([byte]$value)
    }
    return [System.Text.Encoding]::ASCII.GetString($bytes.ToArray())
}

function Read-ExactBytes {
    param([Parameter(Mandatory)] [System.IO.Stream] $Stream, [Parameter(Mandatory)] [int] $Count)
    $buffer = New-Object byte[] $Count
    $read = 0
    while ($read -lt $Count) {
        $chunk = $Stream.Read($buffer, $read, $Count - $read)
        if ($chunk -le 0) { throw "IMAP literal ended after $read of $Count bytes." }
        $read += $chunk
    }
    return ,$buffer
}

function Read-UntilTag {
    param([System.IO.Stream] $Stream, [string] $Tag)
    $lines = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $line = Read-StreamLine -Stream $Stream
        if ($null -eq $line) { throw "IMAP connection closed while waiting for '$Tag'." }
        $lines.Add($line)
        if ($line -match "^$([regex]::Escape($Tag)) (OK|NO|BAD)") { return ,$lines }
    }
}

# ---------------------------------------------------------- DKIM primitives ---

function Get-RelaxedHeader {
    param([Parameter(Mandatory)] [string] $RawText)
    # RawText still contains the CRLF between folded lines.
    $unfolded = $RawText -replace "`r`n", ""
    $colon = $unfolded.IndexOf(':')
    if ($colon -lt 0) { throw "header line has no colon: $RawText" }
    $name = $unfolded.Substring(0, $colon).Trim().ToLowerInvariant()
    $value = $unfolded.Substring($colon + 1)
    $value = $value -replace '[ \t]+', ' '
    $value = $value.Trim()
    return "$name`:$value`r`n"
}

function Get-RelaxedBodyCanonical {
    param([Parameter(Mandatory)] [byte[]] $BodyBytes)
    $text = [System.Text.Encoding]::ASCII.GetString($BodyBytes)
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($line in ($text -split "`r`n")) {
        $value = $line -replace '[ \t]+', ' '
        $value = $value -replace ' +$', ''
        $lines.Add($value)
    }
    while ($lines.Count -gt 0 -and $lines[$lines.Count - 1] -eq '') {
        $lines.RemoveAt($lines.Count - 1)
    }
    if ($lines.Count -eq 0) { return [System.Text.Encoding]::ASCII.GetBytes("`r`n") }
    return [System.Text.Encoding]::ASCII.GetBytes((($lines -join "`r`n") + "`r`n"))
}

function ConvertTo-Sha256Base64 {
    param([byte[]] $Bytes)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { return [Convert]::ToBase64String($sha.ComputeHash($Bytes)) }
    finally { $sha.Dispose() }
}

function Get-Sha256Bytes {
    param([byte[]] $Bytes)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { return ,$sha.ComputeHash($Bytes) }
    finally { $sha.Dispose() }
}

# Windows PowerShell 5.1 (.NET Framework) ships no Ed25519 provider and this
# machine has no OpenSSL, so verification is implemented from RFC 8032 using
# BigInteger field arithmetic. SelfTest() runs RFC 8032 vectors 1 and 2 plus a
# tampered signature before the script trusts any verdict from it.
$ed25519Source = @'
using System;
using System.Numerics;
using System.Security.Cryptography;

public static class TiraEd25519
{
    static readonly BigInteger P = BigInteger.Pow(2, 255) - 19;
    static readonly BigInteger L =
        BigInteger.Pow(2, 252) +
        BigInteger.Parse("27742317777372353535851937790883648493");
    static readonly BigInteger D = BigInteger.Parse(
        "37095705934669439343138083508754565189542113879843219016388785533085940283555");
    static readonly BigInteger SqrtM1 = BigInteger.Parse(
        "19681161376707505956807079304988542015446066515923890162744021073123829784752");
    static readonly BigInteger BaseX = BigInteger.Parse(
        "15112221349535400772501151409588531511454012693041857206046113283949847762202");
    static readonly BigInteger BaseY = BigInteger.Parse(
        "46316835694926478169428394003475163141307993866256225615783033603165251855960");

    struct Point
    {
        public BigInteger X, Y, Z, T;
    }

    static BigInteger Mod(BigInteger a)
    {
        a %= P;
        return a.Sign < 0 ? a + P : a;
    }

    static BigInteger Inverse(BigInteger a)
    {
        return BigInteger.ModPow(Mod(a), P - 2, P);
    }

    static BigInteger FromLittleEndian(byte[] b)
    {
        BigInteger v = BigInteger.Zero;
        for (int i = b.Length - 1; i >= 0; i--)
        {
            v = (v << 8) | b[i];
        }
        return v;
    }

    static Point Identity()
    {
        Point p;
        p.X = BigInteger.Zero;
        p.Y = BigInteger.One;
        p.Z = BigInteger.One;
        p.T = BigInteger.Zero;
        return p;
    }

    static Point BasePoint()
    {
        Point p;
        p.X = BaseX;
        p.Y = BaseY;
        p.Z = BigInteger.One;
        p.T = Mod(BaseX * BaseY);
        return p;
    }

    static Point Add(Point p, Point q)
    {
        BigInteger a = Mod((p.Y - p.X) * (q.Y - q.X));
        BigInteger b = Mod((p.Y + p.X) * (q.Y + q.X));
        BigInteger c = Mod(p.T * 2 * D * q.T);
        BigInteger d = Mod(p.Z * 2 * q.Z);
        BigInteger e = Mod(b - a);
        BigInteger f = Mod(d - c);
        BigInteger g = Mod(d + c);
        BigInteger h = Mod(b + a);
        Point r;
        r.X = Mod(e * f);
        r.Y = Mod(g * h);
        r.T = Mod(e * h);
        r.Z = Mod(f * g);
        return r;
    }

    static Point Double(Point p)
    {
        BigInteger a = Mod(p.X * p.X);
        BigInteger b = Mod(p.Y * p.Y);
        BigInteger c = Mod(2 * p.Z * p.Z);
        BigInteger d = Mod(-a);
        BigInteger e = Mod((p.X + p.Y) * (p.X + p.Y) - a - b);
        BigInteger g = Mod(d + b);
        BigInteger f = Mod(g - c);
        BigInteger h = Mod(d - b);
        Point r;
        r.X = Mod(e * f);
        r.Y = Mod(g * h);
        r.T = Mod(e * h);
        r.Z = Mod(f * g);
        return r;
    }

    static Point Multiply(Point p, BigInteger scalar)
    {
        Point result = Identity();
        Point addend = p;
        while (scalar.Sign > 0)
        {
            if (!scalar.IsEven)
            {
                result = Add(result, addend);
            }
            addend = Double(addend);
            scalar >>= 1;
        }
        return result;
    }

    static byte[] Encode(Point p)
    {
        BigInteger zinv = Inverse(p.Z);
        BigInteger x = Mod(p.X * zinv);
        BigInteger y = Mod(p.Y * zinv);
        byte[] outb = new byte[32];
        BigInteger v = y;
        for (int i = 0; i < 32; i++)
        {
            outb[i] = (byte)(v & 0xFF);
            v >>= 8;
        }
        if (!x.IsEven)
        {
            outb[31] |= 0x80;
        }
        return outb;
    }

    static bool Decode(byte[] encoded, out Point point)
    {
        point = Identity();
        if (encoded.Length != 32)
        {
            return false;
        }
        byte[] yb = (byte[])encoded.Clone();
        int sign = (yb[31] >> 7) & 1;
        yb[31] &= 0x7F;
        BigInteger y = FromLittleEndian(yb);
        if (y >= P)
        {
            return false;
        }
        BigInteger y2 = Mod(y * y);
        BigInteger u = Mod(y2 - 1);
        BigInteger v = Mod(D * y2 + 1);
        BigInteger v3 = Mod(v * v * v);
        BigInteger v7 = Mod(v3 * v3 * v);
        BigInteger x = Mod(u * v3 * BigInteger.ModPow(Mod(u * v7), (P - 5) / 8, P));
        BigInteger vx2 = Mod(v * x * x);
        if (vx2 == u)
        {
            // candidate accepted
        }
        else if (vx2 == Mod(-u))
        {
            x = Mod(x * SqrtM1);
        }
        else
        {
            return false;
        }
        int parity = x.IsEven ? 0 : 1;
        if (parity != sign)
        {
            x = Mod(-x);
        }
        if (x.IsZero && sign == 1)
        {
            return false;
        }
        point.X = x;
        point.Y = y;
        point.Z = BigInteger.One;
        point.T = Mod(x * y);
        return true;
    }

    public static bool Verify(byte[] publicKey, byte[] message, byte[] signature)
    {
        if (publicKey == null || publicKey.Length != 32)
        {
            return false;
        }
        if (signature == null || signature.Length != 64)
        {
            return false;
        }
        Point a;
        if (!Decode(publicKey, out a))
        {
            return false;
        }
        byte[] rBytes = new byte[32];
        Array.Copy(signature, 0, rBytes, 0, 32);
        Point r;
        if (!Decode(rBytes, out r))
        {
            return false;
        }
        byte[] sBytes = new byte[32];
        Array.Copy(signature, 32, sBytes, 0, 32);
        BigInteger s = FromLittleEndian(sBytes);
        if (s >= L)
        {
            return false;
        }
        byte[] hbuf = new byte[32 + 32 + message.Length];
        Array.Copy(rBytes, 0, hbuf, 0, 32);
        Array.Copy(publicKey, 0, hbuf, 32, 32);
        Array.Copy(message, 0, hbuf, 64, message.Length);
        byte[] digest;
        using (SHA512 sha = SHA512.Create())
        {
            digest = sha.ComputeHash(hbuf);
        }
        BigInteger h = FromLittleEndian(digest) % L;
        Point left = Multiply(BasePoint(), s);
        Point right = Add(r, Multiply(a, h));
        byte[] leftBytes = Encode(left);
        byte[] rightBytes = Encode(right);
        for (int i = 0; i < leftBytes.Length; i++)
        {
            if (leftBytes[i] != rightBytes[i])
            {
                return false;
            }
        }
        return true;
    }

    static byte[] Hex(string s)
    {
        byte[] b = new byte[s.Length / 2];
        for (int i = 0; i < b.Length; i++)
        {
            b[i] = Convert.ToByte(s.Substring(i * 2, 2), 16);
        }
        return b;
    }

    public static bool SelfTest()
    {
        byte[] empty = new byte[0];
        byte[] pk1 = Hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
        byte[] sig1 = Hex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155" +
                          "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
        if (!Verify(pk1, empty, sig1))
        {
            return false;
        }
        byte[] bad = (byte[])sig1.Clone();
        bad[0] ^= 0x01;
        if (Verify(pk1, empty, bad))
        {
            return false;
        }
        byte[] pk2 = Hex("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c");
        byte[] sig2 = Hex("92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da" +
                          "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00");
        return Verify(pk2, Hex("72"), sig2);
    }
}
'@

if (-not ('TiraEd25519' -as [type])) {
    Add-Type -TypeDefinition $ed25519Source -ReferencedAssemblies System.Numerics
}
if (-not [TiraEd25519]::SelfTest()) {
    throw "embedded Ed25519 verifier failed its RFC 8032 self-test; refusing to report a DKIM result."
}
Write-Host "  crypto: embedded Ed25519 verifier passed RFC 8032 vectors 1-2 + tamper check" -ForegroundColor Gray

function Read-DerLength {
    param([byte[]] $Bytes, [ref] $Offset)
    $first = $Bytes[$Offset.Value]
    $Offset.Value++
    if ($first -lt 0x80) { return $first }
    $count = $first -band 0x7F
    if ($count -eq 0) { throw "indefinite DER length is not valid here" }
    $length = 0
    for ($i = 0; $i -lt $count; $i++) {
        $length = ($length -shl 8) -bor $Bytes[$Offset.Value]
        $Offset.Value++
    }
    return $length
}

function ConvertFrom-SpkiToRsaParameters {
    param([Parameter(Mandatory)] [string] $Base64Spki)
    $der = [Convert]::FromBase64String($Base64Spki)
    $offset = 0

    if ($der[$offset] -ne 0x30) { throw "SPKI is not a SEQUENCE" }
    $offset++
    [void](Read-DerLength -Bytes $der -Offset ([ref]$offset))

    if ($der[$offset] -ne 0x30) { throw "AlgorithmIdentifier is not a SEQUENCE" }
    $offset++
    $algoLength = Read-DerLength -Bytes $der -Offset ([ref]$offset)
    $offset += $algoLength

    if ($der[$offset] -ne 0x03) { throw "subjectPublicKey is not a BIT STRING" }
    $offset++
    $bitLength = Read-DerLength -Bytes $der -Offset ([ref]$offset)
    $end = $offset + $bitLength
    $offset++ # unused bit count

    if ($der[$offset] -ne 0x30) { throw "RSAPublicKey is not a SEQUENCE" }
    $offset++
    [void](Read-DerLength -Bytes $der -Offset ([ref]$offset))

    if ($der[$offset] -ne 0x02) { throw "RSA modulus is not an INTEGER" }
    $offset++
    $modulusLength = Read-DerLength -Bytes $der -Offset ([ref]$offset)
    $modulus = New-Object byte[] $modulusLength
    [Array]::Copy($der, $offset, $modulus, 0, $modulusLength)
    $offset += $modulusLength

    if ($der[$offset] -ne 0x02) { throw "RSA exponent is not an INTEGER" }
    $offset++
    $exponentLength = Read-DerLength -Bytes $der -Offset ([ref]$offset)
    $exponent = New-Object byte[] $exponentLength
    [Array]::Copy($der, $offset, $exponent, 0, $exponentLength)

    # DER INTEGER is signed: a leading 0x00 only protects the sign bit.
    if ($modulus.Length -gt 1 -and $modulus[0] -eq 0) {
        $modulus = $modulus[1..($modulus.Length - 1)]
    }
    if ($exponent.Length -gt 1 -and $exponent[0] -eq 0) {
        $exponent = $exponent[1..($exponent.Length - 1)]
    }
    if ($offset + $exponentLength -gt $end) { throw "RSA exponent runs past the BIT STRING" }

    return New-Object System.Security.Cryptography.RSAParameters -Property @{
        Modulus  = $modulus
        Exponent = $exponent
    }
}

function Split-HeaderBlock {
    param([Parameter(Mandatory)] [byte[]] $RawBytes)
    $separator = -1
    for ($i = 0; $i -lt ($RawBytes.Length - 3); $i++) {
        if ($RawBytes[$i] -eq 13 -and $RawBytes[$i + 1] -eq 10 -and
            $RawBytes[$i + 2] -eq 13 -and $RawBytes[$i + 3] -eq 10) {
            $separator = $i
            break
        }
    }
    if ($separator -lt 0) { throw "message has no CRLF CRLF header/body separator" }

    $headerBytes = New-Object byte[] $separator
    [Array]::Copy($RawBytes, 0, $headerBytes, 0, $separator)
    $bodyStart = $separator + 4
    $bodyLength = $RawBytes.Length - $bodyStart
    $bodyBytes = New-Object byte[] $bodyLength
    if ($bodyLength -gt 0) { [Array]::Copy($RawBytes, $bodyStart, $bodyBytes, 0, $bodyLength) }

    $headerText = [System.Text.Encoding]::ASCII.GetString($headerBytes)
    $fields = New-Object System.Collections.Generic.List[object]
    foreach ($line in ($headerText -split "`r`n")) {
        if ($fields.Count -gt 0 -and $line -match '^[ \t]') {
            $fields[$fields.Count - 1].RawText += "`r`n" + $line
        } elseif ($line.Length -gt 0) {
            $fields.Add([pscustomobject]@{
                Name    = (($line -split ':', 2)[0]).Trim()
                RawText = $line
            })
        }
    }
    return [pscustomobject]@{ Fields = $fields; Body = $bodyBytes }
}

function Get-DkimTags {
    param([Parameter(Mandatory)] [string] $RawText)
    $unfolded = $RawText -replace "`r`n", ""
    $colon = $unfolded.IndexOf(':')
    $value = $unfolded.Substring($colon + 1)
    $tags = @{}
    foreach ($part in ($value -split ';')) {
        $piece = $part.Trim()
        if ($piece.Length -eq 0) { continue }
        $eq = $piece.IndexOf('=')
        if ($eq -lt 0) { continue }
        $key = $piece.Substring(0, $eq).Trim().ToLowerInvariant()
        $val = $piece.Substring($eq + 1).Trim()
        if ($key -eq 'b' -or $key -eq 'bh' -or $key -eq 'p') {
            $val = $val -replace '\s+', ''
        }
        $tags[$key] = $val
    }
    return $tags
}

function Get-EmptyBHeader {
    param([Parameter(Mandatory)] [string] $RawText)
    # RFC 6376: when computing the signature, the b= value is treated as empty.
    return [regex]::Replace($RawText, '(?i)(?<![A-Za-z0-9_-])b\s*=\s*[^;]*', 'b=')
}

function Get-SignedHeaderData {
    param(
        [Parameter(Mandatory)] $Parsed,
        [Parameter(Mandatory)] [string] $SignatureRaw,
        [Parameter(Mandatory)] [string[]] $HeaderNames
    )

    # The signer signs the *last* instance of each header field. Walk from the
    # bottom of the message for every h= entry, and never reuse an instance.
    $used = New-Object bool[] $Parsed.Fields.Count
    $builder = New-Object System.Text.StringBuilder

    foreach ($wanted in $HeaderNames) {
        $found = $null
        for ($i = $Parsed.Fields.Count - 1; $i -ge 0; $i--) {
            if ($used[$i]) { continue }
            if ($Parsed.Fields[$i].Name -ieq $wanted) {
                $found = $i
                break
            }
        }
        if ($null -eq $found) { throw "signed header '$wanted' is missing from the message" }
        $used[$found] = $true
        [void]$builder.Append((Get-RelaxedHeader -RawText $Parsed.Fields[$found].RawText))
    }

    # RFC 6376 section 3.7, hash step 2: the DKIM-Signature field is
    # canonicalized "and without a trailing CRLF". Every field in h= above ends
    # with CRLF; the signature field must not. Getting this off by one CRLF
    # produces a hash that never matches, which is the exact failure this
    # script was written to surface.
    $signatureLine = Get-RelaxedHeader -RawText (Get-EmptyBHeader -RawText $SignatureRaw)
    $signatureLine = $signatureLine.TrimEnd("`r", "`n")
    [void]$builder.Append($signatureLine)
    return $builder.ToString()
}

# ----------------------------------------------------------- Stalwart lookup ---

function Get-StalwartJson {
    param([string[]] $Arguments)
    if (-not (Test-Path -LiteralPath $StalwartCli)) { throw "stalwart-cli not found: $StalwartCli" }
    $output = & $StalwartCli @Arguments --json 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "stalwart-cli $($Arguments -join ' ') failed: $($output -join ' ')"
    }
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($line in $output) {
        $text = ([string]$line).Trim()
        if ($text.StartsWith("{") -or $text.StartsWith("[")) { $lines.Add($text) }
    }
    if ($lines.Count -eq 0) { return @() }
    return @($lines | ForEach-Object { $_ | ConvertFrom-Json })
}

function Get-DkimKey {
    param($SignatureTags)
    $domainRows = Get-StalwartJson @("query", "Domain", "--fields", "id,name")
    $domain = $domainRows | Where-Object { $_.name -eq $SignatureTags['d'] } | Select-Object -First 1
    if (-not $domain) { throw "sender domain '$($SignatureTags['d'])' does not exist in Stalwart" }

    $keyRows = Get-StalwartJson @(
        "query", "DkimSignature",
        "--fields", "domainId,selector,stage,publicKey,@type",
        "--where", "domainId=$($domain.id)"
    )
    $key = $keyRows | Where-Object {
        $_.selector -eq $SignatureTags['s'] -and $_.stage -eq 'active'
    } | Select-Object -First 1
    return [pscustomobject]@{ Domain = $domain; Key = $key }
}

# ------------------------------------------------------------------- message ---

$messageId = "<dkim-$([guid]::NewGuid().ToString('N'))@$($SenderUser.Split('@')[1])>"
$subject = "DKIM proof   with   repeated spaces"

$headerLines = @(
    "From: $SenderUser"
    "To: $RecipientUser"
    "Subject: $subject"
    "Date: $([DateTime]::UtcNow.ToString('r'))"
    "Message-ID: $messageId"
    "MIME-Version: 1.0"
    "Content-Type: text/plain; charset=us-ascii"
)
# A folded header, a header with trailing whitespace, so relaxed header
# canonicalization cannot be faked by "strip everything".
$headerLines += "X-Tira-Note:   folded   value   "
$headerLines += "`tcontinued on the next line   "

$bodyLines = @(
    "Relaxed body canonicalization check."
    "Trailing spaces follow this period.   "
    "Internal    runs    of    spaces."
    "`tTab-indented line."
    ""
    "Final line."
    ""
    ""
)

$messageText = (($headerLines + @("")) + $bodyLines) -join "`r`n"

# ------------------------------------------------------- 1. submission + AUTH ---
Write-Host "`n[1] Authenticated submission: $SenderUser -> $RecipientUser" -ForegroundColor Cyan
$tcp = New-Object System.Net.Sockets.TcpClient
$tcp.Connect($HostName, $SubmissionPort)
$net = $tcp.GetStream()
$plain = New-TextConnection -Stream $net
Assert-Code -Reply (Read-SmtpReply -Reader $plain.Reader) -Code "220" -Label "greeting"

$plain.Writer.WriteLine("EHLO tira.test")
$ehlo1 = Read-SmtpReply -Reader $plain.Reader
Assert-Code -Reply $ehlo1 -Code "250" -Label "EHLO (cleartext)"
if (-not ($ehlo1 -match "STARTTLS")) { throw "ASSERT FAILED: submission port does not advertise STARTTLS." }

$plain.Writer.WriteLine("STARTTLS")
Assert-Code -Reply (Read-SmtpReply -Reader $plain.Reader) -Code "220" -Label "STARTTLS"

$ssl = New-ProofSslStream -Stream $net -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$ssl.AuthenticateAsClient($ServerName)
Write-Host ("  tls: {0} / cipher {1}" -f $ssl.SslProtocol, $ssl.CipherAlgorithm)
$secure = New-TextConnection -Stream $ssl

$secure.Writer.WriteLine("EHLO tira.test")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "EHLO (TLS)"

$authPlain = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("`0$SenderUser`0$SenderPassword"))
$secure.Writer.WriteLine("AUTH PLAIN $authPlain")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "235" -Label "AUTH PLAIN"

$secure.Writer.WriteLine("MAIL FROM:<$SenderUser>")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "MAIL FROM"
$secure.Writer.WriteLine("RCPT TO:<$RecipientUser>")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "RCPT TO"

$secure.Writer.WriteLine("DATA")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "354" -Label "DATA"

foreach ($line in ($messageText -split "`r`n")) {
    if ($line.StartsWith(".")) { $secure.Writer.WriteLine(".$line") } else { $secure.Writer.WriteLine($line) }
}
$secure.Writer.WriteLine(".")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "message queued"

$secure.Writer.WriteLine("QUIT")
[void](Read-SmtpReply -Reader $secure.Reader)
$ssl.Dispose()
$tcp.Close()
Write-Host "  message-id: $messageId"

# ------------------------------------------------------------- 2. raw IMAP ---
Write-Host "`n[2] Fetching the raw message over IMAP: $RecipientUser" -ForegroundColor Cyan
$tcp2 = New-Object System.Net.Sockets.TcpClient
$tcp2.Connect($HostName, $ImapPort)
$net2 = $tcp2.GetStream()

$greeting = Read-StreamLine -Stream $net2
if ($null -eq $greeting -or -not $greeting.StartsWith("* OK")) {
    throw "IMAP greeting was not * OK: $greeting"
}
$imapWriter = New-Object System.IO.StreamWriter($net2, [System.Text.Encoding]::ASCII, 1)
$imapWriter.NewLine = "`r`n"
$imapWriter.AutoFlush = $true

$imapWriter.WriteLine("p1 STARTTLS")
$reply = Read-UntilTag -Stream $net2 -Tag "p1"
if (-not ($reply -match "^p1 OK")) { throw "IMAP STARTTLS failed: $($reply -join ' | ')" }

$imapSsl = New-ProofSslStream -Stream $net2 -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$imapSsl.AuthenticateAsClient($ServerName)
$imapWriter = New-Object System.IO.StreamWriter($imapSsl, [System.Text.Encoding]::ASCII, 1)
$imapWriter.NewLine = "`r`n"
$imapWriter.AutoFlush = $true

$imapWriter.WriteLine("p2 LOGIN $RecipientUser $RecipientPassword")
$login = Read-UntilTag -Stream $imapSsl -Tag "p2"
if (-not ($login -match "^p2 OK")) { throw "IMAP LOGIN failed for $RecipientUser." }
Write-Host "  ok  IMAP LOGIN" -ForegroundColor Green

$deadline = (Get-Date).AddSeconds($DeliveryTimeoutSeconds)
$folder = $null
$sequence = $null
$tag = 10
while ((Get-Date) -lt $deadline -and -not $folder) {
    foreach ($box in @("INBOX", "Junk Mail", "Sent Items")) {
        $tag++
        $imapWriter.WriteLine("p$tag SELECT `"$box`"")
        $selected = Read-UntilTag -Stream $imapSsl -Tag "p$tag"
        if (-not ($selected -match "^p$tag OK")) { continue }

        $tag++
        $imapWriter.WriteLine("p$tag FETCH 1:* (BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")
        $search = Read-UntilTag -Stream $imapSsl -Tag "p$tag"
        $current = $null
        foreach ($line in $search) {
            if ($line -match '^\* (\d+) FETCH') { $current = $Matches[1] }
            if ($null -ne $current -and $line -match [regex]::Escape($messageId)) {
                $folder = $box
                $sequence = $current
                break
            }
        }
        if ($folder) { break }
    }
    if (-not $folder) { Start-Sleep -Seconds 2 }
}

if (-not $folder) { throw "ASSERT FAILED: message $messageId never appeared for $RecipientUser." }
Write-Host ("  ok  delivered to: {0} (sequence {1})" -f $folder, $sequence) -ForegroundColor Green

$tag++
$fetchTag = "p$tag"
$imapWriter.WriteLine("$fetchTag FETCH $sequence (BODY.PEEK[])")

$literalSize = $null
while ($true) {
    $line = Read-StreamLine -Stream $imapSsl
    if ($null -eq $line) { throw "IMAP closed before the FETCH literal." }
    if ($line -match "^\* \d+ FETCH \(BODY\[\] \{(\d+)\}") {
        $literalSize = [int]$Matches[1]
        break
    }
    if ($line -match "^$([regex]::Escape($fetchTag)) (NO|BAD)") {
        throw "IMAP FETCH failed: $line"
    }
}
$rawBytes = Read-ExactBytes -Stream $imapSsl -Count $literalSize
# Consume the closing ")" and the tagged completion line.
while ($true) {
    $line = Read-StreamLine -Stream $imapSsl
    if ($null -eq $line) { throw "IMAP closed after the FETCH literal." }
    if ($line -match "^$([regex]::Escape($fetchTag)) (OK|NO|BAD)") {
        if ($line -notmatch "^$([regex]::Escape($fetchTag)) OK") { throw "IMAP FETCH did not complete: $line" }
        break
    }
}

$imapWriter.WriteLine("p99 LOGOUT")
[void](Read-UntilTag -Stream $imapSsl -Tag "p99")
$imapSsl.Dispose()
$tcp2.Close()

# The raw message is only needed for offline inspection; writing it to a temp
# file by default would leave message content on disk. Opt in with -KeepRawMessage.
if ($KeepRawMessage) {
    $rawPath = Join-Path ([System.IO.Path]::GetTempPath()) ("dkim-proof-{0}.eml" -f [guid]::NewGuid().ToString('N'))
    [System.IO.File]::WriteAllBytes($rawPath, $rawBytes)
    Write-Host ("  raw message: {0} bytes -> {1}" -f $rawBytes.Length, $rawPath) -ForegroundColor Gray
}

# ------------------------------------------------------------- 3. verify ---
Write-Host "`n[3] Verifying DKIM signatures" -ForegroundColor Cyan

$parsed = Split-HeaderBlock -RawBytes $rawBytes
$signatureFields = @($parsed.Fields | Where-Object { $_.Name -ieq 'DKIM-Signature' })
if ($signatureFields.Count -eq 0) {
    throw "ASSERT FAILED: the delivered message has no DKIM-Signature header."
}

$bodyCanonical = Get-RelaxedBodyCanonical -BodyBytes $parsed.Body
$computedBodyHash = ConvertTo-Sha256Base64 -Bytes $bodyCanonical
Write-Host ("  relaxed body: {0} bytes, sha256={1}" -f $bodyCanonical.Length, $computedBodyHash)

$failures = New-Object System.Collections.Generic.List[string]
$rsaVerified = 0
$ed25519Verified = 0

foreach ($signature in $signatureFields) {
    $tags = Get-DkimTags -RawText $signature.RawText
    $algorithm = $tags['a']
    $selector = $tags['s']
    Write-Host ("`n  signature a={0} s={1} d={2}" -f $algorithm, $selector, $tags['d']) -ForegroundColor Cyan

    if ($tags['bh'] -ne $computedBodyHash) {
        $failures.Add("body hash mismatch for selector $selector`: bh=$($tags['bh']) computed=$computedBodyHash")
        Write-Host "    FAIL body hash does not match bh=" -ForegroundColor Red
        continue
    }
    Write-Host "    ok   body hash matches bh=" -ForegroundColor Green

    $lookup = Get-DkimKey -SignatureTags $tags
    if (-not $lookup.Key) {
        $failures.Add("no active key for selector $selector in domain $($tags['d'])")
        Write-Host "    FAIL no active key in Stalwart for this selector" -ForegroundColor Red
        continue
    }
    Write-Host ("    ok   active key: {0} ({1})" -f $lookup.Key.selector, $lookup.Key.'@type') -ForegroundColor Green

    $signatureBytes = [Convert]::FromBase64String($tags['b'])
    $canonicalization = $tags['c']
    if ($canonicalization -ne 'relaxed/relaxed') {
        $failures.Add("selector $selector uses unsupported canonicalization '$canonicalization'")
        Write-Host "    FAIL canonicalization '$canonicalization' is not implemented (relaxed/relaxed only)" -ForegroundColor Red
        continue
    }

    $signedData = Get-SignedHeaderData -Parsed $parsed -SignatureRaw $signature.RawText -HeaderNames ($tags['h'] -split ':')
    $signedBytes = [System.Text.Encoding]::ASCII.GetBytes($signedData)
    $signedDigest = ConvertTo-Sha256Base64 -Bytes $signedBytes

    if ($algorithm -eq 'rsa-sha256') {
        if ($lookup.Key.'@type' -ne 'Dkim1RsaSha256') {
            $failures.Add("selector $selector is not an RSA key but a=$algorithm")
            Write-Host "    FAIL key type mismatch" -ForegroundColor Red
            continue
        }
        $parameters = ConvertFrom-SpkiToRsaParameters -Base64Spki $lookup.Key.publicKey
        $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider
        try {
            $rsa.ImportParameters($parameters)
            $verified = $rsa.VerifyData(
                $signedBytes,
                $signatureBytes,
                [System.Security.Cryptography.HashAlgorithmName]::SHA256,
                [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
            )
            if (-not $verified) {
                $failures.Add("RSA signature did not verify for selector $selector")
                Write-Host ("    FAIL RSA signature (signed sha256={0})" -f $signedDigest) -ForegroundColor Red
            } else {
                $rsaVerified++
                Write-Host ("    ok   RSA-{0} signature verified (signed sha256={1})" -f ($parameters.Modulus.Length * 8), $signedDigest) -ForegroundColor Green
            }
        } finally {
            $rsa.Dispose()
        }
    } elseif ($algorithm -eq 'ed25519-sha256') {
        if ($lookup.Key.'@type' -ne 'Dkim1Ed25519Sha256') {
            $failures.Add("selector $selector is not an Ed25519 key but a=$algorithm")
            Write-Host "    FAIL key type mismatch" -ForegroundColor Red
            continue
        }
        $publicKeyBytes = [Convert]::FromBase64String($lookup.Key.publicKey)
        if ($publicKeyBytes.Length -ne 32) {
            $failures.Add("Ed25519 key for selector $selector is $($publicKeyBytes.Length) bytes, expected 32")
            Write-Host "    FAIL Ed25519 key length" -ForegroundColor Red
            continue
        }
        if ($signatureBytes.Length -ne 64) {
            $failures.Add("Ed25519 signature for selector $selector is $($signatureBytes.Length) bytes, expected 64")
            Write-Host "    FAIL Ed25519 signature length" -ForegroundColor Red
            continue
        }
        # RFC 8463: ed25519-sha256 signs the SHA-256 digest of the same
        # canonicalized input used for RSA, with the PureEdDSA variant.
        $signedHashBytes = Get-Sha256Bytes -Bytes $signedBytes
        if (-not [TiraEd25519]::Verify($publicKeyBytes, $signedHashBytes, $signatureBytes)) {
            $failures.Add("Ed25519 signature did not verify for selector $selector")
            Write-Host ("    FAIL Ed25519 signature (signed sha256={0})" -f $signedDigest) -ForegroundColor Red
        } else {
            $ed25519Verified++
            Write-Host ("    ok   Ed25519 signature verified (signed sha256={0})" -f $signedDigest) -ForegroundColor Green
        }
    } else {
        $failures.Add("unsupported algorithm '$algorithm' for selector $selector")
        Write-Host "    FAIL unsupported algorithm" -ForegroundColor Red
    }
}

Write-Host ""
if ($failures.Count -gt 0) {
    foreach ($failure in $failures) { Write-Host "  FAIL $failure" -ForegroundColor Red }
    Write-Host "DKIM PROOF FAILED" -ForegroundColor Red
    exit 1
}
if (($rsaVerified + $ed25519Verified) -eq 0) {
    Write-Host "DKIM PROOF FAILED: no signature was mathematically verified" -ForegroundColor Red
    exit 1
}
Write-Host ("DKIM PROOF PASSED: {0} RSA signature(s) verified, {1} Ed25519 signature(s) verified" -f $rsaVerified, $ed25519Verified) -ForegroundColor Green
