#requires -Version 5.1
<#
    Tira Mail - apply / enable / disable the Rspamd Milter in Stalwart.

    Why this exists
    ---------------
    `MtaMilter` is a registry collection, not a singleton. The exact NDJSON
    shape (verified against Stalwart 1.0.2 with `stalwart-cli snapshot`) is:

        {"@type":"upsert","object":"MtaMilter","matchOn":["host"],
         "value":{"mtamilter-rspamd":{...fields...}}}

    Applying a Milter that points at a dead Rspamd with `enable` true and
    `tempFailOnError` true stops inbound mail. So this script:

      1. validates the plan locally with `apply --dry-run`;
      2. refuses `-Enable` unless the Rspamd TCP port actually accepts a
         connection (unless -Force);
      3. applies, then reads the object back and prints what the engine has.

    Failure policy
    --------------
    `-Enable` sets `tempFailOnError = true` (fail closed: 4xx on Rspamd
    failure). Pass `-FailOpen` for a pilot where availability matters more
    than scanning, then remove it once Rspamd has run cleanly for a week.

    Credentials
    -----------
    STALWART_USER / STALWART_PASSWORD come from the environment. A password on
    the command line would be visible in the process list.

    Usage
    -----
        $env:STALWART_URL      = "http://127.0.0.1:8080"
        $env:STALWART_USER     = "admin@tira.test"
        $env:STALWART_PASSWORD = "..."

        .\scripts\milter-apply.ps1                 # apply the disabled plan
        .\scripts\milter-apply.ps1 -Enable         # verify TCP, then enable
        .\scripts\milter-apply.ps1 -Enable -FailOpen
        .\scripts\milter-apply.ps1 -Disable
#>
[CmdletBinding()]
param(
    [string] $Url = "http://127.0.0.1:8080",
    [string] $Cli = "C:\tools\stalwart\stalwart-cli.exe",
    [string] $RspamdHost = "127.0.0.1",
    [ValidateRange(1, 65535)]
    [int] $RspamdPort = 11332,
    [string] $PlanTemplate = "",
    [switch] $Enable,
    [switch] $Disable,
    [switch] $FailOpen,
    [switch] $Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# $PSScriptRoot is empty while param defaults are evaluated under -File, so
# resolve the default here instead (the same trap dns-zone.ps1 documents).
if ([string]::IsNullOrWhiteSpace($PlanTemplate)) {
    $scriptRoot = if ([string]::IsNullOrWhiteSpace($PSScriptRoot)) { (Get-Location).Path } else { $PSScriptRoot }
    $PlanTemplate = Join-Path $scriptRoot "..\deploy\stalwart\milter.ndjson"
}

if ($Enable -and $Disable) { throw "-Enable and -Disable are mutually exclusive" }
if (-not (Test-Path -LiteralPath $Cli)) { throw "stalwart-cli not found: $Cli" }
if (-not (Test-Path -LiteralPath $PlanTemplate)) { throw "plan template not found: $PlanTemplate" }

$user = [Environment]::GetEnvironmentVariable("STALWART_USER")
$password = [Environment]::GetEnvironmentVariable("STALWART_PASSWORD")
if ([string]::IsNullOrWhiteSpace($user) -or [string]::IsNullOrWhiteSpace($password)) {
    throw "set STALWART_USER and STALWART_PASSWORD in the environment first"
}

$env:STALWART_URL = $Url

function Test-TcpPort {
    param([string] $TargetHost, [int] $Port, [int] $TimeoutMs = 3000)
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $task = $client.ConnectAsync($TargetHost, $Port)
        if (-not $task.Wait($TimeoutMs)) { return $false }
        return $client.Connected
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

$plan = Get-Content -Raw -Encoding UTF8 $PlanTemplate | ConvertFrom-Json
$reference = $plan.value.PSObject.Properties | Select-Object -First 1
if (-not $reference) { throw "plan template has an empty value object" }
$milter = $reference.Value

$milter.host = $RspamdHost
$milter.port = $RspamdPort

$state = "disabled"
if ($Enable) {
    if (-not $Force -and -not (Test-TcpPort -TargetHost $RspamdHost -Port $RspamdPort)) {
        throw "nothing is listening on ${RspamdHost}:${RspamdPort}; start Rspamd first (use -Force only to stage an intentionally dead filter)"
    }
    $milter.enable = [pscustomobject]@{ match = [pscustomobject]@{}; else = "true" }
    $milter.tempFailOnError = -not $FailOpen
    $state = if ($FailOpen) { "enabled (fail-open)" } else { "enabled (fail-closed)" }
} elseif ($Disable) {
    $milter.enable = [pscustomobject]@{ match = [pscustomobject]@{}; else = "false" }
    $milter.tempFailOnError = $true
    $state = "disabled"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("tira-milter-{0}.ndjson" -f [guid]::NewGuid())
try {
    $json = $plan | ConvertTo-Json -Depth 12 -Compress
    [IO.File]::WriteAllText($tmp, $json + "`n", (New-Object Text.UTF8Encoding($false)))

    Write-Host "`n[1] Validating the plan locally (no server call)" -ForegroundColor Cyan
    # The summary goes to stderr; capture it so a successful dry run does not
    # look like a failure in a CI log or a PowerShell host that decorates
    # native stderr as an error record.
    # Windows PowerShell 5.1 turns native stderr into a terminating error when
    # it is redirected while $ErrorActionPreference is Stop. Relax it for this
    # one call: stalwart-cli writes its plan summary to stderr by design.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $dryRun = & $Cli apply --file $tmp --dry-run --no-color 2>&1
    $dryExit = $LASTEXITCODE
    $ErrorActionPreference = $previous
    if ($dryExit -ne 0) {
        $dryRun | ForEach-Object { Write-Host $_ }
        throw "plan validation failed"
    }
    Write-Host "  ok  plan parses and validates" -ForegroundColor Green

    Write-Host "`n[2] Applying to $Url" -ForegroundColor Cyan
    Write-Host ("    {0}:{1} -> {2}" -f $RspamdHost, $RspamdPort, $state)
    & $Cli apply --file $tmp --json --no-color
    if ($LASTEXITCODE -ne 0) { throw "milter plan failed (see the engine's error above)" }
} finally {
    Remove-Item $tmp -Force -ErrorAction SilentlyContinue
}

Write-Host "`n[3] Reading the object back from the engine" -ForegroundColor Cyan
# `query` returns only the default columns (host, id, port); the enable
# expression and failure policy need a full `get`.
$rows = & $Cli query MtaMilter --fields id,host,port --json --no-color
if ($LASTEXITCODE -ne 0) { throw "verification query failed" }
$match = $rows | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.host -eq $RspamdHost -and [int]$_.port -eq $RspamdPort } |
    Select-Object -First 1
if (-not $match) { throw "the plan applied but no MtaMilter for ${RspamdHost}:${RspamdPort} was found" }

$detail = (& $Cli get MtaMilter $match.id --json --no-color | Where-Object { $_ } | Select-Object -First 1) | ConvertFrom-Json
if (-not $detail) { throw "could not read back MtaMilter $($match.id)" }

# M-022: compare what the engine stored against what we applied. Printing the
# read-back values without comparing them let a disabled or fail-open milter
# report PASS. The intended values live in $milter (the plan we just applied).
if ($detail.host -ne $RspamdHost -or [int]$detail.port -ne $RspamdPort) {
    throw "milter host/port mismatch: engine has $($detail.host):$($detail.port), applied ${RspamdHost}:${RspamdPort}"
}
$expectedEnable = [string]$milter.enable.else
$actualEnable = [string]$detail.enable.else
if ($actualEnable -ne $expectedEnable) {
    throw "milter enable mismatch: applied '$expectedEnable' but engine has '$actualEnable'; a disabled filter must not report PASS"
}
$expectedTempFail = [bool]$milter.tempFailOnError
$actualTempFail = [bool]$detail.tempFailOnError
if ($actualTempFail -ne $expectedTempFail) {
    throw "milter tempFailOnError mismatch: applied '$expectedTempFail' but engine has '$actualTempFail'; fail-open must not pass as fail-closed"
}

Write-Host ("  ok  id={0} host={1} port={2} enable={3} tempFailOnError={4}" -f `
    $detail.id, $detail.host, $detail.port, $detail.enable.else, $detail.tempFailOnError) -ForegroundColor Green

Write-Host "`nMILTER APPLY: PASS" -ForegroundColor Green
if ($state -like "enabled*") {
    Write-Host "Send a GTUBE test message next; it must be rejected or marked X-Spam." -ForegroundColor Yellow
}
exit 0
