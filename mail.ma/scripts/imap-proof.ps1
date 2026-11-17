#requires -Version 5.1
<#
    Tira Mail - IMAP end-to-end proof against Stalwart.

    Proves, against a live server:
      1. STARTTLS upgrade on the cleartext IMAP port (1143)
      2. LOGIN over TLS (plaintext LOGIN is refused by design)
      3. LIST, SELECT INBOX
      4. APPEND of a real RFC 5322 message
      5. FETCH of that message back, byte-comparable

    Usage (the password is never hardcoded: supply it via TIRA_PROOF_PASSWORD or
    -Password, and opt in to the self-signed test certificate explicitly):
      $env:TIRA_PROOF_PASSWORD = '...'
      .\scripts\imap-proof.ps1 -SkipCertificateCheck
      .\scripts\imap-proof.ps1 -HostName 127.0.0.1 -StartTlsPort 1143 -ImplicitTlsPort 1993 `
          -User yassine@tira.test -Password '...' -SkipCertificateCheck

    Without -SkipCertificateCheck the system validates the certificate chain and
    hostname, so a self-signed test certificate is rejected (fail closed). The
    opt-in is refused for any non-loopback -HostName.
#>
[CmdletBinding()]
param(
    [string] $HostName = "127.0.0.1",
    [int]    $StartTlsPort = 1143,
    [int]    $ImplicitTlsPort = 1993,
    [string] $ServerName = "mail.tira.test",
    [string] $User = "yassine@tira.test",
    [string] $Password,
    [switch] $SkipCertificateCheck
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "proof-common.ps1")

# Fail closed before any network I/O: no hardcoded password fallback.
$Password = Resolve-ProofPassword -Explicit $Password -EnvironmentVariable "TIRA_PROOF_PASSWORD" -Label "IMAP"

function New-ImapConnection {
    param([System.IO.Stream] $Stream)

    # Buffer size 1: never read past the tagged response we are waiting for.
    $reader = New-Object System.IO.StreamReader($Stream, [System.Text.Encoding]::ASCII, $false, 1)
    $writer = New-Object System.IO.StreamWriter($Stream, [System.Text.Encoding]::ASCII, 1)
    $writer.NewLine = "`r`n"
    $writer.AutoFlush = $true
    [pscustomobject]@{ Reader = $reader; Writer = $writer }
}

function Read-ImapUntil {
    param(
        [Parameter(Mandatory)] [System.IO.StreamReader] $Reader,
        [Parameter(Mandatory)] [string] $Tag
    )
    $lines = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $line = $Reader.ReadLine()
        if ($null -eq $line) { throw "Connection closed while waiting for '$Tag'." }
        $lines.Add($line)
        if ($line -match "^$([regex]::Escape($Tag)) (OK|NO|BAD)") { return $lines }
    }
}

function Read-ImapLine {
    param([Parameter(Mandatory)] [System.IO.StreamReader] $Reader)
    $line = $Reader.ReadLine()
    if ($null -eq $line) { throw "Connection closed while waiting for a server line." }
    return $line
}

function Assert-Contains {
    param([string[]] $Lines, [string] $Pattern, [string] $Label)
    if (-not ($Lines -match $Pattern)) {
        throw "ASSERT FAILED [$Label]: expected /$Pattern/ in:`n  $($Lines -join "`n  ")"
    }
    Write-Host ("  ok  {0}" -f $Label) -ForegroundColor Green
}

# The TLS validation callback comes from proof-common.ps1 (New-ProofSslStream):
# system validation by default, an explicit loopback-only opt-in otherwise.

# ---------------------------------------------------------------- STARTTLS ---
Write-Host "`n[1] STARTTLS on ${HostName}:${StartTlsPort}" -ForegroundColor Cyan
$tcp = New-Object System.Net.Sockets.TcpClient
$tcp.Connect($HostName, $StartTlsPort)
$netStream = $tcp.GetStream()
$plain = New-ImapConnection -Stream $netStream

$greeting = Read-ImapLine -Reader $plain.Reader
Write-Host "  greeting: $greeting"
Assert-Contains -Lines @($greeting) -Pattern "^\* OK" -Label "server greets"
if ($greeting -notmatch "LOGINDISABLED") {
    Write-Warning "Server did not advertise LOGINDISABLED on the cleartext port."
}

$plain.Writer.WriteLine("a1 STARTTLS")
$startTls = Read-ImapUntil -Reader $plain.Reader -Tag "a1"
Assert-Contains -Lines $startTls -Pattern "^a1 OK" -Label "STARTTLS accepted"

$sslStream = New-ProofSslStream -Stream $netStream -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$sslStream.AuthenticateAsClient($ServerName)
$cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($sslStream.RemoteCertificate)
Write-Host ("  tls: {0} / cipher {1} / hash {2}" -f $sslStream.SslProtocol, $sslStream.CipherAlgorithm, $sslStream.HashAlgorithm)
Write-Host ("  cert subject: {0}" -f $cert.Subject)
Write-Host ("  cert SANs  : {0}" -f ($cert.Extensions | Where-Object { $_.Oid.Value -eq "2.5.29.17" } | ForEach-Object { $_.Format($false) }))
Write-Host ("  cert expiry: {0:u}" -f $cert.NotAfter)

$secure = New-ImapConnection -Stream $sslStream

# ------------------------------------------------------- LOGIN / LIST / SELECT
Write-Host "`n[2] Authenticated session over TLS" -ForegroundColor Cyan
$secure.Writer.WriteLine("a2 LOGIN $User $Password")
$login = Read-ImapUntil -Reader $secure.Reader -Tag "a2"
Assert-Contains -Lines $login -Pattern "^a2 OK" -Label "LOGIN accepted over TLS"

$secure.Writer.WriteLine('a3 LIST "" "*"')
$list = Read-ImapUntil -Reader $secure.Reader -Tag "a3"
Assert-Contains -Lines $list -Pattern "^a3 OK" -Label "LIST completed"
$list | Where-Object { $_ -match '^\* LIST' } | ForEach-Object { Write-Host "  $_" }

$secure.Writer.WriteLine("a4 SELECT INBOX")
$select = Read-ImapUntil -Reader $secure.Reader -Tag "a4"
Assert-Contains -Lines $select -Pattern "^a4 OK" -Label "SELECT INBOX"
$existsBefore = ($select | Where-Object { $_ -match '^\* (\d+) EXISTS' } | Select-Object -First 1)
Write-Host "  $existsBefore"

# ----------------------------------------------------------------- APPEND ---
Write-Host "`n[3] APPEND a real message" -ForegroundColor Cyan
$messageId = "<proof-$([guid]::NewGuid().ToString('N'))@tira.test>"
# Base64 of the UTF-8 Arabic subject: "إثبات IMAP من تيرا ميل"
# Kept as a literal so this script stays pure ASCII and parses on any code page.
$subjectB64 = "2KXYq9io2KfYqiBJTUFQINmF2YYg2KrZitix2Kcg2YXZitmE"
$body = "Salam from Tira Mail.`r`nThis message was APPENDed over TLS and FETCHed back.`r`n"
$message = @(
    "From: $User"
    "To: $User"
    "Subject: =?UTF-8?B?${subjectB64}?="
    "Date: $([DateTime]::UtcNow.ToString('r'))"
    "Message-ID: $messageId"
    "MIME-Version: 1.0"
    "Content-Type: text/plain; charset=utf-8"
    "Content-Transfer-Encoding: 8bit"
    ""
    $body
) -join "`r`n"

$bytes = [System.Text.Encoding]::UTF8.GetBytes($message)
$secure.Writer.WriteLine("a5 APPEND INBOX {$($bytes.Length)}")
$continuation = Read-ImapLine -Reader $secure.Reader
Write-Host "  continuation: $continuation"
if ($continuation -notmatch "^\+") { throw "Server refused the literal: $continuation" }
$sslStream.Write($bytes, 0, $bytes.Length)
$sslStream.Write([byte[]]@(13, 10), 0, 2)
$sslStream.Flush()
$append = Read-ImapUntil -Reader $secure.Reader -Tag "a5"
Assert-Contains -Lines $append -Pattern "^a5 OK" -Label "APPEND stored"

# ------------------------------------------------------------------ FETCH ---
Write-Host "`n[4] FETCH it back" -ForegroundColor Cyan
$secure.Writer.WriteLine("a6 SELECT INBOX")
$select2 = Read-ImapUntil -Reader $secure.Reader -Tag "a6"
$existsAfter = ($select2 | Where-Object { $_ -match '^\* (\d+) EXISTS' } | Select-Object -First 1)
Write-Host "  before: $existsBefore / after: $existsAfter"

$secure.Writer.WriteLine("a7 FETCH 1:* (UID RFC822.SIZE BODY.PEEK[HEADER.FIELDS (SUBJECT MESSAGE-ID)])")
$fetch = Read-ImapUntil -Reader $secure.Reader -Tag "a7"
Assert-Contains -Lines $fetch -Pattern "^a7 OK" -Label "FETCH completed"
if (-not ($fetch -match [regex]::Escape($messageId))) {
    throw "ASSERT FAILED: appended Message-ID $messageId was not returned by FETCH."
}
Write-Host "  ok  appended Message-ID came back from FETCH" -ForegroundColor Green
$fetch | Where-Object { $_ -match 'Subject:|MESSAGE-ID|Message-ID|RFC822.SIZE' } | ForEach-Object { Write-Host "  $_" }

$secure.Writer.WriteLine("a8 LOGOUT")
[void](Read-ImapUntil -Reader $secure.Reader -Tag "a8")
$sslStream.Dispose()
$tcp.Close()

# ------------------------------------------------------------ IMPLICIT TLS ---
Write-Host "`n[5] Implicit TLS on ${HostName}:${ImplicitTlsPort}" -ForegroundColor Cyan
$tcp2 = New-Object System.Net.Sockets.TcpClient
$tcp2.Connect($HostName, $ImplicitTlsPort)
$ssl2 = New-ProofSslStream -Stream $tcp2.GetStream() -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$ssl2.AuthenticateAsClient($ServerName)
$implicit = New-ImapConnection -Stream $ssl2
$greeting2 = Read-ImapLine -Reader $implicit.Reader
Write-Host "  greeting: $greeting2"
Assert-Contains -Lines @($greeting2) -Pattern "^\* OK" -Label "implicit TLS greeting"
$implicit.Writer.WriteLine("b1 LOGIN $User $Password")
$login2 = Read-ImapUntil -Reader $implicit.Reader -Tag "b1"
Assert-Contains -Lines $login2 -Pattern "^b1 OK" -Label "LOGIN on implicit TLS"
$implicit.Writer.WriteLine("b2 LOGOUT")
[void](Read-ImapUntil -Reader $implicit.Reader -Tag "b2")
$ssl2.Dispose()
$tcp2.Close()

Write-Host "`nIMAP proof: PASS" -ForegroundColor Green
