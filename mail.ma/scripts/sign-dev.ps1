#requires -Version 5.1
<#
    Tira Mail - sign locally built executables for Smart App Control.

    Why this exists
    ---------------
    Windows Smart App Control (SAC) is enforced on this development machine.
    When it flipped from evaluation to enforcement, every freshly linked
    unsigned .exe was refused at exec time with:

        Une strategie de controle d'application a bloque ce fichier. (os error 4551)

    That includes cargo test binaries and build scripts, so `cargo test` fails
    with "could not execute process ... (never executed)" even though the build
    succeeded. Older binaries keep working, which makes the failure look random.

    This script creates a local code-signing certificate once, trusts it in the
    CurrentUser stores, and signs the requested executables. A locally trusted
    signature is enough for SAC to allow them.

    Why -Force exists
    -----------------
    SAC can keep refusing a file whose signature is *valid*: the verdict is
    cached per file hash. Signing is deterministic, so stripping and re-signing
    the same bytes reproduces the blocked hash. `-Force` removes the old
    certificate table, appends 32 random bytes of PE overlay (the loader never
    reads them), then signs. That produces a genuinely new hash, and SAC allows
    it. Use it only on the specific binary SAC refused.

    This is a development-machine workaround, not a product requirement. On a
    normal CI runner (or with SAC off) it is unnecessary.

    Usage
    -----
        .\scripts\sign-dev.ps1                    # target\debug + target\debug\deps
        .\scripts\sign-dev.ps1 -Path target\debug\tira-provision.exe
        .\scripts\sign-dev.ps1 -Since (Get-Date).AddMinutes(-10)

    After signing, `cargo test` runs normally again. Rebuilds need re-signing.
#>
[CmdletBinding()]
param(
    [string[]] $Path,
    [datetime] $Since = [datetime]::MinValue,
    # RFC 3161 timestamp authority. SAC accepted signed-but-untimestamped
    # binaries for a while and then started refusing some of them with
    # "did not meet the Enterprise signing level requirements"; a timestamped
    # signature was accepted. Requires network access; the script falls back
    # to an untimestamped signature when the authority cannot be reached.
    [string] $TimestampServer = 'http://timestamp.digicert.com',
    [switch] $Force,
    # H-10: creating the cert adds an exportable code-signing root to
    # CurrentUser\Root and TrustedPublisher (permanent trust weakening).
    # Require explicit consent; never do it implicitly.
    [switch] $AllowTrustStore,
    # H-10: reversible cleanup. Removes the dev cert from My/Root/TrustedPublisher.
    [switch] $RemoveTrust
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# H-10: a development-machine workaround for Smart App Control, never a
# product/CI/server step. Refuse on any build agent: weakening the trust store
# there would persist beyond the job and affect every later build.
foreach ($marker in @("CI", "GITHUB_ACTIONS", "TF_BUILD", "BUILD_BUILDID", "JENKINS_URL")) {
    if ([Environment]::GetEnvironmentVariable($marker)) {
        throw "sign-dev.ps1 refuses to run on a CI/build agent ($marker is set). It is a local development-machine workaround only."
    }
}

$subject = "CN=Tira Dev Builds"

# H-10: reversible cleanup. Remove the dev cert from every store it was added
# to, so the trust weakening can be undone. Safe when the cert is absent.
if ($RemoveTrust) {
    $removed = 0
    foreach ($storeName in @("My", "Root", "TrustedPublisher")) {
        $store = New-Object System.Security.Cryptography.X509Certificates.X509Store($storeName, "CurrentUser")
        $store.Open("ReadWrite")
        try {
            $found = @($store.Certificates | Where-Object { $_.Subject -eq $subject })
            foreach ($c in $found) { $store.Remove($c); $removed++ }
        } finally {
            $store.Close()
        }
    }
    Write-Host ("removed {0} 'Tira Dev Builds' certificate(s) from CurrentUser stores" -f $removed) -ForegroundColor Green
    return
}

$cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
    Where-Object { $_.Subject -eq $subject -and $_.NotAfter -gt (Get-Date).AddDays(30) } |
    Select-Object -First 1

if (-not $cert) {
    if (-not $AllowTrustStore) {
        throw "No trusted '$subject' certificate found. Creating one adds an exportable code-signing root to CurrentUser\Root and TrustedPublisher (permanent trust weakening). Re-run with -AllowTrustStore on your development machine only, or clean up an old one with -RemoveTrust."
    }
    Write-Host "Creating local code-signing certificate ($subject)" -ForegroundColor Cyan
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $subject `
        -CertStoreLocation Cert:\CurrentUser\My `
        -KeyExportPolicy Exportable `
        -NotAfter (Get-Date).AddYears(5)

    foreach ($storeName in @("Root", "TrustedPublisher")) {
        $store = New-Object System.Security.Cryptography.X509Certificates.X509Store($storeName, "CurrentUser")
        $store.Open("ReadWrite")
        $store.Add($cert)
        $store.Close()
    }
}

if (-not $Path -or $Path.Count -eq 0) {
    $Path = @(
        (Join-Path $PSScriptRoot "..\target\debug"),
        (Join-Path $PSScriptRoot "..\target\debug\deps")
    )
}

$files = foreach ($item in $Path) {
    if (Test-Path $item -PathType Leaf) {
        Get-Item $item
    } elseif (Test-Path $item) {
        Get-ChildItem $item -Filter *.exe -File
    }
}

# Removes the PE certificate table (data directory entry 4) and returns the
# file offset where it started, or $null if the file is not a PE image. The
# signature covers everything before that table, so any content change must
# start here, not at the end of an already-signed file.
function Remove-PeCertificateTable {
    param([Parameter(Mandatory)] [string] $FilePath)

    $bytes = [System.IO.File]::ReadAllBytes($FilePath)
    if ($bytes.Length -lt 64) { return $null }

    $eLfanew = [BitConverter]::ToInt32($bytes, 0x3C)
    if ($eLfanew -le 0 -or ($eLfanew + 24) -gt $bytes.Length) { return $null }
    if ($bytes[$eLfanew] -ne 0x50 -or $bytes[$eLfanew + 1] -ne 0x45) { return $null }

    $optional = $eLfanew + 24
    $magic = [BitConverter]::ToUInt16($bytes, $optional)
    if ($magic -eq 0x20B) {
        $dataDirectory = $optional + 112
    } elseif ($magic -eq 0x10B) {
        $dataDirectory = $optional + 96
    } else {
        return $null
    }

    $securityEntry = $dataDirectory + (4 * 8)
    if (($securityEntry + 8) -gt $bytes.Length) { return $null }

    $certOffset = [BitConverter]::ToInt32($bytes, $securityEntry)
    $certSize = [BitConverter]::ToInt32($bytes, $securityEntry + 4)
    if ($certOffset -le 0 -or $certSize -le 0) { return $null }
    if (($certOffset + $certSize) -gt $bytes.Length) { return $null }

    [System.Array]::Clear($bytes, $securityEntry, 8)
    $stream = [System.IO.File]::Open($FilePath, 'Open', 'Write')
    try {
        $stream.SetLength($certOffset)
        $stream.Position = 0
        $stream.Write($bytes, 0, $certOffset)
    } finally {
        $stream.Close()
    }
    return $certOffset
}

# Appends bytes after the PE image. The Windows loader ignores this overlay,
# but it changes the file hash, which is what SAC keys its verdict on.
function Add-PeOverlay {
    param(
        [Parameter(Mandatory)] [string] $FilePath,
        [int] $Length = 32
    )
    $overlay = New-Object byte[] $Length
    $rng = New-Object System.Security.Cryptography.RNGCryptoServiceProvider
    try {
        $rng.GetBytes($overlay)
    } finally {
        $rng.Dispose()
    }
    $stream = [System.IO.File]::Open($FilePath, 'Append', 'Write')
    try {
        $stream.Write($overlay, 0, $overlay.Length)
    } finally {
        $stream.Close()
    }
}

$signed = 0
$skipped = 0
foreach ($file in $files) {
    if ($file.Extension -ne ".exe") { continue }
    if ($file.LastWriteTime -lt $Since) { continue }

    $existing = Get-AuthenticodeSignature -FilePath $file.FullName
    $needsSignature = -not ($existing.Status -eq "Valid" -and $existing.SignerCertificate.Subject -eq $subject)

    if (-not $Force -and -not $needsSignature) {
        $skipped++
        continue
    }

    if ($Force) {
        # Strip first: modifying an already-signed file in place makes
        # Set-AuthenticodeSignature fail with UnknownError.
        [void](Remove-PeCertificateTable -FilePath $file.FullName)
        Add-PeOverlay -FilePath $file.FullName
    }

    $result = Set-AuthenticodeSignature -FilePath $file.FullName -Certificate $cert -HashAlgorithm SHA256 -TimestampServer $TimestampServer
    if ($result.Status -ne "Valid" -and $TimestampServer) {
        # Offline or the TSA is down: a local signature is still better than
        # nothing, and SAC sometimes accepts it.
        Write-Warning ("timestamp failed for {0} ({1}); signing without one" -f $file.Name, $result.Status)
        $result = Set-AuthenticodeSignature -FilePath $file.FullName -Certificate $cert -HashAlgorithm SHA256
    }
    if ($result.Status -ne "Valid") {
        Write-Warning ("signing failed for {0}: {1}" -f $file.Name, $result.Status)
        continue
    }
    $signed++
}

Write-Host ("signed {0} file(s), {1} already valid" -f $signed, $skipped) -ForegroundColor Green
