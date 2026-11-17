#requires -Version 5.1
<#
    Standalone tests for the proof-script secret-hygiene guards in proof-common.ps1.

    No Stalwart, no database, no network: this exercises Resolve-ProofPassword,
    Test-LoopbackHost, Get-ProofCertificateCallback, New-ProofSslStream and
    Set-ProofRestCertificatePolicy directly, so the exact fail-closed behaviour the
    *-proof.ps1 scripts rely on is proven independently of a live engine. The
    SslStream checks run over a MemoryStream (never authenticated); the REST policy
    checks save and restore the process-wide ServicePointManager policy so they
    cannot leak state into other tests.

    Run:
      powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/proof-hygiene.Tests.ps1

    Exit 0 = all passed; non-zero = at least one failure (printed in red).
#>

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "..\proof-common.ps1")

$script:passes = 0
$script:failures = 0
function Check {
    param([Parameter(Mandatory)][string] $Name, [Parameter(Mandatory)][scriptblock] $Body)
    try {
        & $Body
        $script:passes++
        Write-Host "  ok  $Name" -ForegroundColor Green
    } catch {
        $script:failures++
        Write-Host "  FAIL $Name : $_" -ForegroundColor Red
    }
}

# Save the process-wide TLS policy and the test environment variable so no check
# can leak state into another check or into the caller's session.
$origPolicy = [System.Net.ServicePointManager]::CertificatePolicy
$envName = "TIRA_PROOF_HYGIENE_TEST_PW"
$origEnv = [Environment]::GetEnvironmentVariable($envName)

try {
    # ---------------------------------------------------- Resolve-ProofPassword ---
    Check "Resolve-ProofPassword: an explicit argument wins over the environment" {
        [Environment]::SetEnvironmentVariable($envName, "from-env")
        try {
            $pw = Resolve-ProofPassword -Explicit "from-arg" -EnvironmentVariable $envName -Label "test"
            Assert-True ($pw -eq "from-arg") "expected from-arg, got '$pw'"
        } finally { [Environment]::SetEnvironmentVariable($envName, $null) }
    }

    Check "Resolve-ProofPassword: the environment is used when no explicit argument" {
        [Environment]::SetEnvironmentVariable($envName, "env-secret")
        try {
            $pw = Resolve-ProofPassword -Explicit "" -EnvironmentVariable $envName
            Assert-True ($pw -eq "env-secret") "expected env-secret, got '$pw'"
        } finally { [Environment]::SetEnvironmentVariable($envName, $null) }
    }

    Check "Resolve-ProofPassword: a missing password fails closed, no hardcoded fallback" {
        [Environment]::SetEnvironmentVariable($envName, $null)
        $threw = $false; $msg = ""
        try { Resolve-ProofPassword -Explicit "" -EnvironmentVariable $envName -Label "IMAP" | Out-Null }
        catch { $threw = $true; $msg = "$_" }
        Assert-True $threw "must throw when no password is available"
        Assert-True ($msg -match "refusing to fall back to a hardcoded secret") "message must state the fail-closed reason: $msg"
        Assert-True ($msg -notmatch "TiraTest2026") "must not echo the old hardcoded secret"
    }

    Check "Resolve-ProofPassword: whitespace-only values are treated as absent" {
        [Environment]::SetEnvironmentVariable($envName, "   ")
        try {
            $threw = $false
            try { Resolve-ProofPassword -Explicit "  " -EnvironmentVariable $envName | Out-Null } catch { $threw = $true }
            Assert-True $threw "whitespace-only must fail closed"
        } finally { [Environment]::SetEnvironmentVariable($envName, $null) }
    }

    Check "Resolve-ProofPassword: a password with meaningful spaces is returned untrimmed" {
        $pw = Resolve-ProofPassword -Explicit " secret with spaces " -EnvironmentVariable $envName
        Assert-True ($pw -eq " secret with spaces ") "leading/trailing spaces must be preserved"
    }

    # ------------------------------------------------------- Test-LoopbackHost ---
    Check "Test-LoopbackHost accepts the loopback literals" {
        foreach ($h in @("127.0.0.1", "127.0.0.2", "::1", "[::1]", "localhost", "LOCALHOST", " 127.0.0.1 ")) {
            Assert-True (Test-LoopbackHost -TargetHost $h) "expected loopback: '$h'"
        }
    }

    Check "Test-LoopbackHost rejects names and routable addresses" {
        foreach ($h in @("mail.tira.test", "example.com", "8.8.8.8", "192.168.1.10", "10.0.0.1", "::2")) {
            Assert-True (-not (Test-LoopbackHost -TargetHost $h)) "expected NOT loopback: '$h'"
        }
    }

    # --------------------------------------------- Get-ProofCertificateCallback ---
    Check "Get-ProofCertificateCallback: no opt-in means system validation for any host" {
        foreach ($h in @("127.0.0.1", "mail.tira.test", "8.8.8.8")) {
            $cb = Get-ProofCertificateCallback -SkipCertificateCheck $false -TargetHost $h
            Assert-True ($null -eq $cb) "no opt-in must return null (system validation) for '$h'"
        }
    }

    Check "Get-ProofCertificateCallback: a loopback opt-in returns a trust-all delegate" {
        $cb = Get-ProofCertificateCallback -SkipCertificateCheck $true -TargetHost "127.0.0.1"
        Assert-True ($null -ne $cb) "opt-in must return a callback"
        Assert-True ($cb -is [System.Net.Security.RemoteCertificateValidationCallback]) "must be the SslStream delegate type"
        $verdict = $cb.Invoke($null, $null, $null, [System.Net.Security.SslPolicyErrors]::RemoteCertificateNameMismatch)
        Assert-True ($verdict -eq $true) "trust-all callback must accept despite a name-mismatch error"
    }

    Check "Get-ProofCertificateCallback: a remote opt-in is refused (fail closed)" {
        foreach ($h in @("mail.tira.test", "8.8.8.8", "example.com")) {
            $threw = $false
            try { Get-ProofCertificateCallback -SkipCertificateCheck $true -TargetHost $h | Out-Null } catch { $threw = $true }
            Assert-True $threw "opt-in against '$h' must throw"
        }
    }

    # ------------------------------------------------------ New-ProofSslStream ---
    Check "New-ProofSslStream: no opt-in builds a default-validation stream" {
        $ms = New-Object System.IO.MemoryStream
        $ssl = New-ProofSslStream -Stream $ms -SkipCertificateCheck $false -TargetHost "127.0.0.1"
        try { Assert-True ($ssl -is [System.Net.Security.SslStream]) "must return an SslStream" }
        finally { $ssl.Dispose() }
    }

    Check "New-ProofSslStream: a loopback opt-in builds a trust-all stream" {
        $ms = New-Object System.IO.MemoryStream
        $ssl = New-ProofSslStream -Stream $ms -SkipCertificateCheck $true -TargetHost "::1"
        try { Assert-True ($ssl -is [System.Net.Security.SslStream]) "must return an SslStream" }
        finally { $ssl.Dispose() }
    }

    Check "New-ProofSslStream: a remote opt-in throws before constructing" {
        $ms = New-Object System.IO.MemoryStream
        $threw = $false
        try { New-ProofSslStream -Stream $ms -SkipCertificateCheck $true -TargetHost "mail.tira.test" | Out-Null }
        catch { $threw = $true }
        finally { $ms.Dispose() }
        Assert-True $threw "remote opt-in must throw"
    }

    # --------------------------------------------- Set-ProofRestCertificatePolicy ---
    Check "Set-ProofRestCertificatePolicy: no opt-in leaves the process policy unchanged" {
        $before = [System.Net.ServicePointManager]::CertificatePolicy
        $prev = Set-ProofRestCertificatePolicy -SkipCertificateCheck $false -BaseUrl "https://127.0.0.1:8443"
        Assert-True ([object]::ReferenceEquals($before, [System.Net.ServicePointManager]::CertificatePolicy)) "no opt-in must not install a policy"
        Assert-True ([object]::ReferenceEquals($before, $prev)) "must return the unchanged previous policy"
    }

    Check "Set-ProofRestCertificatePolicy: a loopback opt-in installs compiled trust-all and returns the previous" {
        $before = [System.Net.ServicePointManager]::CertificatePolicy
        $prev = Set-ProofRestCertificatePolicy -SkipCertificateCheck $true -BaseUrl "https://127.0.0.1:8443"
        try {
            $now = [System.Net.ServicePointManager]::CertificatePolicy
            Assert-True ($null -ne $now) "a policy must be installed"
            Assert-True ($now.GetType().Name -eq "TiraProofTrustAllCertificates") "compiled trust-all must be installed, got $($now.GetType().Name)"
            Assert-True ([object]::ReferenceEquals($before, $prev)) "must return the previous policy for restore"
        } finally { [System.Net.ServicePointManager]::CertificatePolicy = $prev }
    }

    Check "Set-ProofRestCertificatePolicy: a remote opt-in is refused and leaves the policy untouched" {
        $before = [System.Net.ServicePointManager]::CertificatePolicy
        $threw = $false
        try { Set-ProofRestCertificatePolicy -SkipCertificateCheck $true -BaseUrl "https://mail.example.com:8443" | Out-Null } catch { $threw = $true }
        Assert-True $threw "remote opt-in must throw"
        Assert-True ([object]::ReferenceEquals($before, [System.Net.ServicePointManager]::CertificatePolicy)) "policy must be unchanged after refusal"
    }
}
finally {
    [System.Net.ServicePointManager]::CertificatePolicy = $origPolicy
    [Environment]::SetEnvironmentVariable($envName, $origEnv)
}

Write-Host ""
Write-Host ("proof-hygiene tests: {0} passed, {1} failed" -f $script:passes, $script:failures)
if ($script:failures -gt 0) { exit 1 }
Write-Host "PROOF HYGIENE TESTS: PASS" -ForegroundColor Green
exit 0