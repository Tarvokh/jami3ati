#requires -Version 5.1
<#
    Standalone tests for the H-08 engine-backup helpers in backup-common.ps1.

    No Stalwart, no database: this builds a dummy RocksDB-shaped directory and
    exercises Get-FileInventory, Test-RocksDbStructure, Test-ManagementListener
    and Test-EngineStopped. The stop guard is proven deterministically with a
    .NET TcpListener (open/closed port) and a held LOCK file handle, so the exact
    fail-closed behaviour engine-backup.ps1 relies on is tested independently of
    a live engine.

    Run:
      powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/engine-backup.Tests.ps1

    Exit 0 = all passed; non-zero = at least one failure (printed in red).
#>

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "..\backup-common.ps1")

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

function Get-ClosedPort {
    $l = New-Object System.Net.Sockets.TcpListener ([Net.IPAddress]::Loopback), 0
    $l.Start()
    $p = $l.LocalEndpoint.Port
    $l.Stop()
    return $p
}

$root = Join-Path ([IO.Path]::GetTempPath()) ("tira-h08-test-" + [guid]::NewGuid().ToString("N"))
$data = Join-Path $root "data"
New-Item -ItemType Directory -Force -Path $data | Out-Null
try {
    # A minimal but complete RocksDB-shaped directory.
    [IO.File]::WriteAllText((Join-Path $root "config.json"), '{"@type":"RocksDb","path":"data"}')
    [IO.File]::WriteAllText((Join-Path $data "CURRENT"), "MANIFEST-000001`n")
    [IO.File]::WriteAllBytes((Join-Path $data "IDENTITY"), [Text.Encoding]::ASCII.GetBytes("dummy-identity"))
    [IO.File]::WriteAllBytes((Join-Path $data "MANIFEST-000001"), [Text.Encoding]::ASCII.GetBytes("dummy-manifest-bytes"))
    [IO.File]::WriteAllBytes((Join-Path $data "OPTIONS-000001"), [Text.Encoding]::ASCII.GetBytes("dummy-options"))
    [IO.File]::WriteAllBytes((Join-Path $data "LOCK"), [byte[]] @())
    [IO.File]::WriteAllBytes((Join-Path $data "000001.sst"), [Text.Encoding]::ASCII.GetBytes("dummy-sst"))
    $absentProc = "tira_no_such_proc_" + [guid]::NewGuid().ToString("N")

    Check "Get-FileInventory counts files and sums bytes" {
        $inv = Get-FileInventory -Root $root
        $onDisk = @(Get-ChildItem -LiteralPath $root -Recurse -File)
        $sum = ($onDisk | Measure-Object -Property Length -Sum).Sum
        Assert-True ($inv.Count -eq $onDisk.Count) "count=$($inv.Count), expected $($onDisk.Count)"
        Assert-True ($inv.TotalBytes -eq $sum) "bytes=$($inv.TotalBytes), expected $sum"
    }

    Check "Get-FileInventory uses relative forward-slash names and correct hashes" {
        $inv = Get-FileInventory -Root $root
        $current = $inv.Files | Where-Object { $_.name -eq "data/CURRENT" }
        Assert-True ($null -ne $current) "data/CURRENT not inventoried with forward slashes"
        Assert-True ($current.sha256 -eq (Get-Sha256 (Join-Path $data "CURRENT"))) "CURRENT hash mismatch"
        foreach ($e in $inv.Files) {
            Assert-True ($e.name -notmatch '\\') "name has a backslash: $($e.name)"
            Assert-True ($e.name -notmatch '^[A-Za-z]:') "name is absolute: $($e.name)"
        }
    }

    Check "Test-RocksDbStructure accepts a complete store" {
        $r = Test-RocksDbStructure -DataDir $data
        Assert-True ($null -eq $r) "expected null (ok), got: $r"
    }

    Check "Test-RocksDbStructure rejects missing CURRENT" {
        $cur = Join-Path $data "CURRENT"
        $bak = [IO.File]::ReadAllText($cur)
        Remove-Item -LiteralPath $cur -Force
        try { Assert-True ((Test-RocksDbStructure -DataDir $data) -match 'CURRENT') "missing CURRENT must be reported" }
        finally { [IO.File]::WriteAllText($cur, $bak) }
    }

    Check "Test-RocksDbStructure rejects CURRENT pointing at a missing MANIFEST" {
        $cur = Join-Path $data "CURRENT"
        $bak = [IO.File]::ReadAllText($cur)
        [IO.File]::WriteAllText($cur, "MANIFEST-999999`n")
        try { Assert-True ((Test-RocksDbStructure -DataDir $data) -match 'missing MANIFEST-999999') "dangling MANIFEST must be reported" }
        finally { [IO.File]::WriteAllText($cur, $bak) }
    }

    Check "Test-RocksDbStructure rejects garbage CURRENT" {
        $cur = Join-Path $data "CURRENT"
        $bak = [IO.File]::ReadAllText($cur)
        [IO.File]::WriteAllText($cur, "not-a-manifest`n")
        try { Assert-True ((Test-RocksDbStructure -DataDir $data) -match 'does not name a MANIFEST') "garbage CURRENT must be reported" }
        finally { [IO.File]::WriteAllText($cur, $bak) }
    }

    Check "Test-RocksDbStructure rejects missing IDENTITY" {
        $id = Join-Path $data "IDENTITY"
        $bytes = [IO.File]::ReadAllBytes($id)
        Remove-Item -LiteralPath $id -Force
        try { Assert-True ((Test-RocksDbStructure -DataDir $data) -match 'IDENTITY') "missing IDENTITY must be reported" }
        finally { [IO.File]::WriteAllBytes($id, $bytes) }
    }

    Check "Test-RocksDbStructure rejects no OPTIONS file" {
        $opt = Join-Path $data "OPTIONS-000001"
        $bytes = [IO.File]::ReadAllBytes($opt)
        Remove-Item -LiteralPath $opt -Force
        try { Assert-True ((Test-RocksDbStructure -DataDir $data) -match 'OPTIONS') "missing OPTIONS must be reported" }
        finally { [IO.File]::WriteAllBytes($opt, $bytes) }
    }

    Check "Test-ManagementListener: an open port is reachable" {
        $l = New-Object System.Net.Sockets.TcpListener ([Net.IPAddress]::Loopback), 0
        $l.Start()
        try {
            $p = $l.LocalEndpoint.Port
            Assert-True (Test-ManagementListener -Url "http://127.0.0.1:$p") "a listening port must be reachable"
        } finally { $l.Stop() }
    }

    Check "Test-ManagementListener: a closed port is not reachable" {
        $p = Get-ClosedPort
        Assert-True (-not (Test-ManagementListener -Url "http://127.0.0.1:$p")) "a closed port must not be reachable"
    }

    Check "Test-EngineStopped: no process + closed port + free LOCK => stopped" {
        $p = Get-ClosedPort
        $stopped = Test-EngineStopped -ProcessName $absentProc -ManagementUrl "http://127.0.0.1:$p" -DataDir $data
        Assert-True $stopped "all signals down must mean stopped"
    }

    Check "Test-EngineStopped: a reachable listener => NOT stopped (fail closed)" {
        $l = New-Object System.Net.Sockets.TcpListener ([Net.IPAddress]::Loopback), 0
        $l.Start()
        try {
            $p = $l.LocalEndpoint.Port
            $stopped = Test-EngineStopped -ProcessName $absentProc -ManagementUrl "http://127.0.0.1:$p" -DataDir $data
            Assert-True (-not $stopped) "a reachable management listener must mean running"
        } finally { $l.Stop() }
    }

    Check "Test-EngineStopped: a held RocksDB LOCK => NOT stopped (fail closed)" {
        $p = Get-ClosedPort
        $lock = Join-Path $data "LOCK"
        $holder = [IO.File]::Open($lock, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        try {
            $stopped = Test-EngineStopped -ProcessName $absentProc -ManagementUrl "http://127.0.0.1:$p" -DataDir $data
            Assert-True (-not $stopped) "a held LOCK must mean the engine is running"
        } finally { $holder.Close() }
    }
}
finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host ("engine-backup tests: {0} passed, {1} failed" -f $script:passes, $script:failures)
if ($script:failures -gt 0) { exit 1 }
Write-Host "ENGINE BACKUP TESTS: PASS" -ForegroundColor Green
exit 0