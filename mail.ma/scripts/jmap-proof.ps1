#requires -Version 5.1
<#
    Tira Mail - JMAP proof against Stalwart.

    Proves:
      1. JMAP session discovery over HTTPS with Basic auth
      2. The authenticated session exposes a real mail account
      3. Mailbox/get lists the account mailboxes
      4. Email/query + Email/get read real messages, with the Arabic
         RFC 2047 subject decoded to UTF-8 by the server

    Usage (the password is never hardcoded: supply it via TIRA_PROOF_PASSWORD or
    -Password, and opt in to the self-signed test certificate explicitly):
      $env:TIRA_PROOF_PASSWORD = '...'
      .\scripts\jmap-proof.ps1 -SkipCertificateCheck

    Without -SkipCertificateCheck the system validates the certificate chain and
    hostname, so a self-signed test certificate is rejected (fail closed). The
    opt-in is refused for any non-loopback -BaseUrl host, and the process-wide
    certificate policy is always restored in a finally block.
#>
[CmdletBinding()]
param(
    [string] $BaseUrl = "https://127.0.0.1:8443",
    [string] $User = "yassine@tira.test",
    [string] $Password,
    [switch] $SkipCertificateCheck
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "proof-common.ps1")

# Fail closed before any network I/O: no hardcoded password fallback.
$Password = Resolve-ProofPassword -Explicit $Password -EnvironmentVariable "TIRA_PROOF_PASSWORD" -Label "JMAP"

# Invoke-RestMethod validates TLS on a .NET worker thread (no runspace), so the
# opt-in uses a compiled ICertificatePolicy. Set-ProofRestCertificatePolicy
# installs it only for a loopback opt-in and returns the previous policy, which
# the finally block restores so it is never left process-wide.
$previousPolicy = Set-ProofRestCertificatePolicy -SkipCertificateCheck:$SkipCertificateCheck -BaseUrl $BaseUrl
try {
    $pair = "${User}:${Password}"
    $auth = "Basic " + [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($pair))
    $headers = @{ Authorization = $auth }

    Write-Host "`n[1] Session discovery: $BaseUrl/jmap/session" -ForegroundColor Cyan
    # Requested directly: .NET drops the Authorization header when following the
    # 307 from /.well-known/jmap to /jmap/session, which silently yields an
    # anonymous session.
    $session = Invoke-RestMethod -Uri "$BaseUrl/jmap/session" -Headers $headers -Method Get -TimeoutSec 15
    Write-Host ("  username : {0}" -f $session.username)
    Write-Host ("  apiUrl   : {0}" -f $session.apiUrl)
    Write-Host ("  state    : {0}" -f $session.state)

    if (-not $session.username) { throw "ASSERT FAILED: session is anonymous - Basic auth was not accepted." }
    Write-Host "  ok  authenticated session" -ForegroundColor Green

    $accountIds = @($session.accounts.PSObject.Properties.Name)
    if ($accountIds.Count -eq 0) { throw "ASSERT FAILED: no accounts in session." }
    $accountId = $accountIds[0]
    Write-Host ("  ok  account {0} ({1})" -f $accountId, $session.accounts.$accountId.name) -ForegroundColor Green

    $capabilities = @($session.capabilities.PSObject.Properties.Name)
    foreach ($needed in @("urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail")) {
        if ($capabilities -notcontains $needed) { throw "ASSERT FAILED: missing capability $needed" }
    }
    Write-Host ("  ok  {0} capabilities advertised" -f $capabilities.Count) -ForegroundColor Green

    # The session advertises the public hostname; point it back at this test listener.
    $apiUrl = $session.apiUrl -replace "^https://[^/]+", $BaseUrl
    Write-Host "  apiUrl (rewritten for local test): $apiUrl"

    function Invoke-Jmap {
        param([Parameter(Mandatory)] [object[]] $MethodCalls)
        $body = @{
            using      = @("urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail")
            methodCalls = $MethodCalls
        } | ConvertTo-Json -Depth 12 -Compress
        Invoke-RestMethod -Uri $apiUrl -Headers $headers -Method Post -Body $body -ContentType "application/json" -TimeoutSec 20
    }

    function Get-JmapResponse {
        param([Parameter(Mandatory)] $Response, [Parameter(Mandatory)] [string] $Method)
        foreach ($mr in $Response.methodResponses) {
            if ($mr[0] -eq $Method) { return $mr[1] }
        }
        throw ("ASSERT FAILED: no '{0}' response in: {1}" -f $Method, ($Response | ConvertTo-Json -Depth 8 -Compress))
    }

    Write-Host "`n[2] Mailbox/get" -ForegroundColor Cyan
    $mailboxes = Invoke-Jmap -MethodCalls @(
        ,@("Mailbox/get", @{ accountId = $accountId; ids = $null }, "c1")
    )
    $mb = Get-JmapResponse -Response $mailboxes -Method "Mailbox/get"
    if ($mb.PSObject.Properties.Name -contains "error") { throw "Mailbox/get failed: $($mb.error | ConvertTo-Json -Compress)" }
    $inbox = $mb.list | Where-Object { $_.role -eq "inbox" } | Select-Object -First 1
    if (-not $inbox) { throw "ASSERT FAILED: no mailbox with role inbox." }
    Write-Host ("  ok  {0} mailboxes, INBOX id = {1}" -f @($mb.list).Count, $inbox.id) -ForegroundColor Green
    $mb.list | ForEach-Object { Write-Host ("    {0,-14} {1}" -f $_.name, $_.id) }

    Write-Host "`n[3] Email/query + Email/get (single request, result reference c2 -> c3)" -ForegroundColor Cyan
    $read = Invoke-Jmap -MethodCalls @(
        ,@("Email/query", @{ accountId = $accountId; filter = @{ inMailbox = $inbox.id }; sort = @(@{ property = "receivedAt"; isAscending = $false }) }, "c2")
        ,@("Email/get", @{
            accountId  = $accountId
            "#ids"     = @{ resultOf = "c2"; name = "Email/query"; path = "/ids" }
            properties = @("subject", "from", "receivedAt", "keywords", "size")
        }, "c3")
    )
    $q = Get-JmapResponse -Response $read -Method "Email/query"
    $g = Get-JmapResponse -Response $read -Method "Email/get"
    if ($q.PSObject.Properties.Name -contains "error") { throw "Email/query failed: $($q | ConvertTo-Json -Compress)" }
    if ($g.PSObject.Properties.Name -contains "error") { throw "Email/get failed: $($g | ConvertTo-Json -Compress)" }

    $ids = @($q.ids)
    $total = if ($q.PSObject.Properties.Name -contains "total") { $q.total } else { $ids.Count }
    Write-Host ("  ok  {0} message(s) in INBOX, total {1}" -f $ids.Count, $total) -ForegroundColor Green
    if ($ids.Count -eq 0) { throw "ASSERT FAILED: INBOX is empty over JMAP although IMAP showed messages." }
    Write-Host ("  ok  Email/get returned {0} object(s)" -f @($g.list).Count) -ForegroundColor Green

    $decodedArabic = $false
    foreach ($email in $g.list) {
        $from = if ($email.from -and @($email.from).Count -gt 0) { @($email.from)[0].email } else { "?" }
        Write-Host ("    [{0}] {1}  <- {2}" -f $email.id, $email.subject, $from)
        if ($email.subject -match "[\u0600-\u06FF]") { $decodedArabic = $true }
    }

    if ($decodedArabic) {
        Write-Host "  ok  Arabic subject arrived as decoded UTF-8 (no RFC 2047 residue)" -ForegroundColor Green
    } else {
        Write-Warning "No Arabic subject found in the sampled messages; RFC 2047 decoding not proven in this run."
    }

    Write-Host "`nJMAP proof: PASS" -ForegroundColor Green
}
finally {
    [System.Net.ServicePointManager]::CertificatePolicy = $previousPolicy
}
