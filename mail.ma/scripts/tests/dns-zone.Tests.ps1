#requires -Version 5.1
<#
    Standalone tests for DNS-zone honesty: the pure mail-host sanity check in
    dns-common.ps1, and the committed demo artifacts under deploy/dns.

    No Stalwart, no database, no network. Run:
      powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/dns-zone.Tests.ps1

    Exit 0 = all passed; non-zero = at least one failure (printed in red).
#>

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "..\dns-common.ps1")

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
function Assert-True {
    param([Parameter(Mandatory)][bool] $Condition, [string] $Message = "assertion failed")
    if (-not $Condition) { throw $Message }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$dnsDir = Join-Path $repoRoot "deploy\dns"

# ------------------------------------------------- pure Get-MailHostFinding ---
Check "in-domain mail host is sane (tira.test -> mail.tira.test)" {
    $f = Get-MailHostFinding -MxText "tira.test. IN MX 10 mail.tira.test." -DomainName "tira.test"
    Assert-True ($null -eq $f) "expected no finding for an in-domain host, got: $($f | ConvertTo-Json -Compress)"
}

Check "borrowed external host warns (uit.ac.ma -> mail.tira.test)" {
    $f = Get-MailHostFinding -MxText "uit.ac.ma. IN MX 10 mail.tira.test." -DomainName "uit.ac.ma"
    Assert-True ($null -ne $f) "expected a finding for an out-of-domain host"
    Assert-True ($f.level -eq "warn") "expected warn, got $($f.level)"
    Assert-True ($f.code -eq "mail-host-external") "expected mail-host-external, got $($f.code)"
}

Check "an explicit -MailHost that matches the shared host is accepted" {
    $f = Get-MailHostFinding -MxText "uit.ac.ma. IN MX 10 mail.tira.test." -DomainName "uit.ac.ma" -ExpectedHost "mail.tira.test"
    Assert-True ($null -eq $f) "operator pinned the shared host; expected no finding"
}

Check "an explicit -MailHost that differs warns (mismatch)" {
    $f = Get-MailHostFinding -MxText "uit.ac.ma. IN MX 10 mail.tira.test." -DomainName "uit.ac.ma" -ExpectedHost "mail.uit.ac.ma"
    Assert-True ($null -ne $f -and $f.code -eq "mail-host-mismatch") "expected mail-host-mismatch, got: $($f | ConvertTo-Json -Compress)"
}

Check "an apex MX target counts as in-domain" {
    $f = Get-MailHostFinding -MxText "tira.test. IN MX 10 tira.test." -DomainName "tira.test"
    Assert-True ($null -eq $f) "apex target must be treated as in-domain"
}

Check "a malformed MX returns null (the presence check owns it)" {
    $f = Get-MailHostFinding -MxText "tira.test. IN MX" -DomainName "tira.test"
    Assert-True ($null -eq $f) "must not invent a target from a malformed MX"
}

Check "host comparison ignores case and the trailing dot" {
    $f = Get-MailHostFinding -MxText "UIT.AC.MA. IN MX 10 MAIL.TIRA.TEST." -DomainName "uit.ac.ma"
    Assert-True ($null -ne $f -and $f.code -eq "mail-host-external") "normalisation must still flag the external host"
}

# ------------------------------------------- committed artifact honesty ---
Check "tira.test artifact zoneFile is portable (basename, no machine path)" {
    $json = Get-Content (Join-Path $dnsDir "tira.test.dns.json") -Raw | ConvertFrom-Json
    Assert-True ($json.zoneFile -eq "tira.test.zone") "zoneFile should be 'tira.test.zone', got '$($json.zoneFile)'"
    Assert-True ($json.zoneFile -notmatch '[\\/]') "zoneFile must not contain a path separator: '$($json.zoneFile)'"
    Assert-True ($json.zoneFile -notmatch '^[A-Za-z]:') "zoneFile must not be a drive-absolute path: '$($json.zoneFile)'"
}

Check "uit.ac.ma artifact zoneFile is portable (basename, no machine path)" {
    $json = Get-Content (Join-Path $dnsDir "uit.ac.ma.dns.json") -Raw | ConvertFrom-Json
    Assert-True ($json.zoneFile -eq "uit.ac.ma.zone") "zoneFile should be 'uit.ac.ma.zone', got '$($json.zoneFile)'"
    Assert-True ($json.zoneFile -notmatch '[\\/]') "zoneFile must not contain a path separator: '$($json.zoneFile)'"
    Assert-True ($json.zoneFile -notmatch '^[A-Za-z]:') "zoneFile must not be a drive-absolute path: '$($json.zoneFile)'"
}

Check "tira.test artifact has no error or warn findings (proof domain stays clean)" {
    $json = Get-Content (Join-Path $dnsDir "tira.test.dns.json") -Raw | ConvertFrom-Json
    $bad = @($json.findings | Where-Object { $_.level -ne "ok" })
    Assert-True ($bad.Count -eq 0) "tira.test must have only ok findings, found: $($bad | ConvertTo-Json -Compress)"
}

Check "uit.ac.ma artifact records the external-host warning" {
    $json = Get-Content (Join-Path $dnsDir "uit.ac.ma.dns.json") -Raw | ConvertFrom-Json
    $warn = @($json.findings | Where-Object { $_.code -eq "mail-host-external" })
    Assert-True ($warn.Count -eq 1) "expected exactly one mail-host-external finding"
    Assert-True ($warn[0].level -eq "warn") "the external-host finding must be a warning"
}

Check "the committed uit.ac.ma warning matches the pure function's output" {
    $zonePath = Join-Path $dnsDir "uit.ac.ma.zone"
    $mxLine = @(Get-Content $zonePath | Where-Object { $_ -match '\bIN\s+MX\b' })[0]
    $f = Get-MailHostFinding -MxText $mxLine -DomainName "uit.ac.ma"
    $json = Get-Content (Join-Path $dnsDir "uit.ac.ma.dns.json") -Raw | ConvertFrom-Json
    $committed = @($json.findings | Where-Object { $_.code -eq "mail-host-external" })[0]
    Assert-True ($null -ne $f) "the real uit.ac.ma MX must produce a finding"
    Assert-True ($f.level -eq $committed.level -and $f.code -eq $committed.code) "committed finding must match the generator logic"
    Assert-True ($f.message -eq $committed.message) "committed message must match the generator logic exactly"
}

# ------------------------------------------------------- zone demo headers ---
Check "tira.test.zone carries a demo header comment" {
    $first = @(Get-Content (Join-Path $dnsDir "tira.test.zone") | Where-Object { $_.Trim() -ne "" })[0]
    Assert-True ($first.TrimStart().StartsWith(";")) "first non-empty line must be a ';' comment, got: $first"
    Assert-True ($first -match 'demo') "header must mark the zone as demo: $first"
}

Check "uit.ac.ma.zone marks the real-looking domain as demo/example, not a deployment" {
    $head = @(Get-Content (Join-Path $dnsDir "uit.ac.ma.zone") | Where-Object { $_.TrimStart().StartsWith(";") }) -join " "
    Assert-True ($head -match 'DEMO') "header must say DEMO: $head"
    Assert-True ($head -match 'uit\.ac\.ma') "header must name the domain: $head"
    Assert-True ($head -match 'not') "header must state it is NOT a real deployment: $head"
}

Write-Host ""
Write-Host ("dns-zone tests: {0} passed, {1} failed" -f $script:passes, $script:failures)
if ($script:failures -gt 0) { exit 1 }
Write-Host "DNS ZONE TESTS: PASS" -ForegroundColor Green
exit 0