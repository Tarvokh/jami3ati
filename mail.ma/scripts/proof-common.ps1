#requires -Version 5.1
<#
    Shared secret-hygiene and TLS guards for the Tira Mail *-proof.ps1 scripts.

    Dot-source this file; it defines functions only and has no load-time side
    effects (the one Add-Type is lazy, inside the function that needs it).

    The proof scripts talk to a live Stalwart that, in the test environment,
    presents a self-signed certificate and uses real account passwords. Two
    mistakes are easy to make and dangerous to ship in a repository:

      1. a hardcoded default password that ends up in git history and CI logs;
      2. an unconditional "trust every certificate" callback that silently
         disables TLS validation even when the script is pointed at a remote
         host, which is a man-in-the-middle hole.

    Both decisions live here so a standalone test (proof-hygiene.Tests.ps1) can
    prove the fail-closed behaviour without a running engine. The rule throughout
    is: default to the secure path, and make the insecure path an explicit opt-in
    that is refused for anything except a loopback target.
#>

Set-StrictMode -Version Latest

function Assert-True {
    param(
        [Parameter(Mandatory)] [bool] $Condition,
        [Parameter(Mandatory)] [string] $Message
    )
    if (-not $Condition) { throw $Message }
}

# Resolves a proof password without ever defaulting to a hardcoded secret.
# Precedence: an explicit -Password argument, then the named environment
# variable, then fail closed (throw). An empty or whitespace-only value at either
# step is treated as absent, so a blank -Password cannot mask a missing secret.
# The original (untrimmed) value is returned, because a real password may carry
# meaningful leading/trailing spaces; only the emptiness test trims. This function
# never writes the password to the host, to a file, or to argv.
function Resolve-ProofPassword {
    param(
        [string] $Explicit,
        [Parameter(Mandatory)] [string] $EnvironmentVariable,
        [string] $Label = "proof"
    )
    if ($Explicit -and $Explicit.Trim()) { return $Explicit }
    $fromEnv = [Environment]::GetEnvironmentVariable($EnvironmentVariable)
    if ($fromEnv -and $fromEnv.Trim()) { return $fromEnv }
    throw "no $Label password: pass -Password or set `$env:$EnvironmentVariable (refusing to fall back to a hardcoded secret)"
}

# True only for a loopback target. The insecure-TLS opt-in is refused for any
# other host: disabling certificate validation against a remote server is never a
# convenience. Uses IPAddress.IsLoopback so the whole 127.0.0.0/8 range and ::1
# are covered, accepts the bracketed IPv6 literal "[::1]", and accepts the
# conventional "localhost" name. A DNS name or a routable IP is not loopback.
function Test-LoopbackHost {
    param([Parameter(Mandatory)] [string] $TargetHost)
    $h = $TargetHost.Trim()
    if ($h.StartsWith('[') -and $h.EndsWith(']')) { $h = $h.Substring(1, $h.Length - 2) }
    if ($h -ieq 'localhost') { return $true }
    $ip = $null
    if ([System.Net.IPAddress]::TryParse($h, [ref] $ip)) {
        return [System.Net.IPAddress]::IsLoopback($ip)
    }
    return $false
}

# Builds the SslStream certificate-validation callback for a proof connection.
# Default (no -SkipCertificateCheck): returns $null, which makes SslStream use the
# operating system's normal chain + hostname validation. Opt-in
# (-SkipCertificateCheck): allowed ONLY against a loopback host, emits a loud
# warning, and returns a callback that accepts the self-signed test certificate.
# Requesting the opt-in against a non-loopback host throws (fail closed) instead
# of silently disabling validation on a remote server. The returned scriptblock
# references no parent-scope variables, so it is safe to invoke synchronously from
# SslStream.AuthenticateAsClient on the calling thread.
function Get-ProofCertificateCallback {
    param(
        [bool] $SkipCertificateCheck,
        [Parameter(Mandatory)] [string] $TargetHost
    )
    if (-not $SkipCertificateCheck) { return $null }
    if (-not (Test-LoopbackHost -TargetHost $TargetHost)) {
        throw "-SkipCertificateCheck is only allowed against a loopback host (got '$TargetHost'); refusing to disable TLS validation on a remote server"
    }
    Write-Warning "TLS certificate validation is DISABLED for loopback host '$TargetHost' (self-signed test certificate). Never use this against a production or remote server."
    return [System.Net.Security.RemoteCertificateValidationCallback] {
        param($sender, $certificate, $chain, $errors)
        return $true
    }
}

# Wraps the SslStream constructor so the secure default does not depend on how
# PowerShell binds a $null delegate. With no opt-in it uses the two-argument
# constructor (system validation); with a loopback opt-in it passes the trust-all
# callback. The caller still runs AuthenticateAsClient($ServerName) itself.
function New-ProofSslStream {
    param(
        [Parameter(Mandatory)] [System.IO.Stream] $Stream,
        [bool] $SkipCertificateCheck,
        [Parameter(Mandatory)] [string] $TargetHost
    )
    $callback = Get-ProofCertificateCallback -SkipCertificateCheck $SkipCertificateCheck -TargetHost $TargetHost
    if ($null -eq $callback) {
        return (New-Object System.Net.Security.SslStream($Stream, $false))
    }
    return (New-Object System.Net.Security.SslStream($Stream, $false, $callback))
}

# jmap-proof.ps1 uses Invoke-RestMethod, whose TLS validation runs on a .NET
# worker thread with no PowerShell runspace, so a scriptblock callback cannot be
# used there; a compiled ICertificatePolicy is required. This installs that
# policy ONLY for an explicit loopback opt-in, and returns the previous policy so
# the caller can restore it in a finally block (it must never be left
# process-wide). With no opt-in it installs nothing and returns the current policy
# unchanged, so default .NET validation stays in force. A non-loopback opt-in
# throws (fail closed) and leaves the policy untouched.
function Set-ProofRestCertificatePolicy {
    param(
        [bool] $SkipCertificateCheck,
        [Parameter(Mandatory)] [string] $BaseUrl
    )
    $previous = [System.Net.ServicePointManager]::CertificatePolicy
    if (-not $SkipCertificateCheck) { return $previous }
    $uri = [Uri] $BaseUrl
    if (-not (Test-LoopbackHost -TargetHost $uri.Host)) {
        throw "-SkipCertificateCheck is only allowed against a loopback host (got '$($uri.Host)'); refusing to disable TLS validation on a remote server"
    }
    if (-not ('TiraProofTrustAllCertificates' -as [type])) {
        Add-Type -TypeDefinition @"
using System.Net;
using System.Security.Cryptography.X509Certificates;
public sealed class TiraProofTrustAllCertificates : ICertificatePolicy
{
    public bool CheckValidationResult(ServicePoint sp, X509Certificate cert, WebRequest req, int problem) { return true; }
}
"@
    }
    Write-Warning "TLS certificate validation is DISABLED for loopback host '$($uri.Host)' (self-signed test certificate). Never use this against a production or remote server."
    [System.Net.ServicePointManager]::CertificatePolicy = New-Object TiraProofTrustAllCertificates
    return $previous
}