#requires -Version 5.1
<#
    Standalone tests for the M-21 backup-integrity helpers in backup-common.ps1.

    No database, no Rspamd, no engine: this exercises Test-ContentAddress and
    Get-BlobPreflightState against a throwaway content-addressed blob root with
    synthetic "key|size" rows, so the exact categorisation backup.ps1 relies on
    is proven independently of PostgreSQL.

    Run:
      powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/backup-integrity.Tests.ps1

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

# Writes content at its content address (xx/yy/<sha256>) under $root.
function Put-Blob {
    param([Parameter(Mandatory)][string] $Root, [Parameter(Mandatory)][string] $Content)
    $bytes = [Text.Encoding]::UTF8.GetBytes($Content)
    $tmp = Join-Path $Root "_tmp"
    [IO.File]::WriteAllBytes($tmp, $bytes)
    $hash = Get-Sha256 $tmp
    $key = "{0}/{1}/{2}" -f $hash.Substring(0, 2), $hash.Substring(2, 2), $hash
    $dir = Join-Path $Root ($key.Substring(0, 5) -replace '/', '\')
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $path = Join-Path $Root ($key -replace '/', '\')
    [IO.File]::WriteAllBytes($path, $bytes)
    Remove-Item -LiteralPath $tmp -Force
    return [pscustomobject]@{ Key = $key; Path = $path; Size = $bytes.Length; Content = $Content }
}

$root = Join-Path ([IO.Path]::GetTempPath()) ("tira-m21-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $root | Out-Null
try {
    $good = Put-Blob -Root $root -Content "hello tira mail"
    $size = $good.Size

    Check "Test-ContentAddress passes for an intact blob" {
        Assert-True (Test-ContentAddress -Path $good.Path -Key $good.Key) "intact blob must verify"
    }

    Check "Test-ContentAddress fails for same-size corruption (bit-rot/poison)" {
        $bytes = [IO.File]::ReadAllBytes($good.Path)
        $bytes[0] = $bytes[0] -bxor 0xFF
        [IO.File]::WriteAllBytes($good.Path, $bytes)
        Assert-True (-not (Test-ContentAddress -Path $good.Path -Key $good.Key)) "same-size poison must fail"
        [IO.File]::WriteAllBytes($good.Path, [Text.Encoding]::UTF8.GetBytes($good.Content))
    }

    Check "Test-ContentAddress fails for a missing file" {
        Assert-True (-not (Test-ContentAddress -Path (Join-Path $root "nope") -Key $good.Key)) "missing file must fail"
    }

    Check "Test-ContentAddress throws on an invalid key" {
        $threw = $false
        try { Test-ContentAddress -Path $good.Path -Key "not-a-key" | Out-Null } catch { $threw = $true }
        Assert-True $threw "invalid key must throw"
    }

    Check "Get-BlobPreflightState: intact blob is clean and counted" {
        $s = Get-BlobPreflightState -BlobRoot $root -Rows @("{0}|{1}" -f $good.Key, $size)
        Assert-True ($s.Checked -eq 1) "checked=$($s.Checked), expected 1"
        Assert-True ($s.Missing.Count -eq 0 -and $s.WrongSize.Count -eq 0 -and $s.Corrupt.Count -eq 0) "intact must be clean"
    }

    Check "Get-BlobPreflightState: missing blob is categorized, not counted" {
        $missingKey = "ab/cd/" + ("0" * 64)
        $s = Get-BlobPreflightState -BlobRoot $root -Rows @("{0}|5" -f $missingKey)
        Assert-True ($s.Missing.Count -eq 1) "missing=$($s.Missing.Count), expected 1"
        Assert-True ($s.Checked -eq 0) "a missing blob is not 'checked'"
    }

    Check "Get-BlobPreflightState: wrong size is wrong_size, not corrupt" {
        $s = Get-BlobPreflightState -BlobRoot $root -Rows @("{0}|{1}" -f $good.Key, ($size + 99))
        Assert-True ($s.WrongSize.Count -eq 1) "wrong_size=$($s.WrongSize.Count), expected 1"
        Assert-True ($s.Corrupt.Count -eq 0) "wrong size must not also count as corrupt"
    }

    Check "Get-BlobPreflightState: same-size corruption is corrupt, not wrong_size" {
        $bytes = [IO.File]::ReadAllBytes($good.Path)
        $bytes[0] = $bytes[0] -bxor 0xFF
        [IO.File]::WriteAllBytes($good.Path, $bytes)
        $s = Get-BlobPreflightState -BlobRoot $root -Rows @("{0}|{1}" -f $good.Key, $size)
        Assert-True ($s.Corrupt.Count -eq 1) "corrupt=$($s.Corrupt.Count), expected 1"
        Assert-True ($s.WrongSize.Count -eq 0) "same-size poison must not be wrong_size"
        [IO.File]::WriteAllBytes($good.Path, [Text.Encoding]::UTF8.GetBytes($good.Content))
    }

    Check "Get-BlobPreflightState: empty rows are clean" {
        $s = Get-BlobPreflightState -BlobRoot $root -Rows @()
        Assert-True ($s.Checked -eq 0 -and $s.Missing.Count -eq 0 -and $s.Corrupt.Count -eq 0) "empty must be clean"
    }

    Check "Get-BlobPreflightState: invalid key throws" {
        $threw = $false
        try { Get-BlobPreflightState -BlobRoot $root -Rows @("bad|5") | Out-Null } catch { $threw = $true }
        Assert-True $threw "invalid key must throw"
    }

    Check "Resolve-Tar returns an existing tar executable" {
        $tar = Resolve-Tar
        Assert-True (Test-Path -LiteralPath $tar -PathType Leaf) "Resolve-Tar returned a non-existent path: $tar"
    }

    Check "Resolve-Tar ignores a GNU-style tar earlier in PATH (prefers System32 bsdtar)" {
        if (-not $env:SystemRoot) { return }
        $sys = Join-Path $env:SystemRoot "System32\tar.exe"
        if (-not (Test-Path -LiteralPath $sys -PathType Leaf)) { return }
        # Plant a decoy tar.exe first in PATH. `Get-Command tar.exe` would return
        # it; Resolve-Tar must still prefer the native Windows bsdtar, because a
        # GNU tar misreads "C:\..." as a remote host and breaks the archive.
        $decoyDir = Join-Path $root "decoy-bin"
        New-Item -ItemType Directory -Force -Path $decoyDir | Out-Null
        $decoy = Join-Path $decoyDir "tar.exe"
        [IO.File]::WriteAllBytes($decoy, [byte[]] (0x4D, 0x5A))
        $oldPath = $env:PATH
        try {
            $env:PATH = "$decoyDir;$oldPath"
            $tar = Resolve-Tar
            Assert-True ($tar -ieq $sys) "Resolve-Tar picked '$tar' instead of System32 bsdtar '$sys'"
        } finally {
            $env:PATH = $oldPath
        }
    }

    # ---- PGPASSWORD hygiene (#45): the secret must not outlive the operation ---
    # Save the caller's PGPASSWORD so these checks cannot leak state into other
    # checks or into the operator's session.
    $origPg = [Environment]::GetEnvironmentVariable("PGPASSWORD")
    try {
        Check "ConvertTo-SafeDatabaseUrl moves the password out of argv into PGPASSWORD" {
            [Environment]::SetEnvironmentVariable("PGPASSWORD", $null)
            $safe = ConvertTo-SafeDatabaseUrl "postgres://user:s3cret%21@127.0.0.1:55432/db"
            Assert-True ($safe -eq "postgres://user@127.0.0.1:55432/db") "safe URL must drop the password: $safe"
            Assert-True ([Environment]::GetEnvironmentVariable("PGPASSWORD") -eq "s3cret!") "PGPASSWORD must hold the decoded password"
        }

        Check "ConvertTo-SafeDatabaseUrl leaves a password-free URL and PGPASSWORD untouched" {
            [Environment]::SetEnvironmentVariable("PGPASSWORD", "pre-existing")
            $safe = ConvertTo-SafeDatabaseUrl "postgres://user@127.0.0.1:55432/db"
            Assert-True ($safe -eq "postgres://user@127.0.0.1:55432/db") "password-free URL unchanged"
            Assert-True ([Environment]::GetEnvironmentVariable("PGPASSWORD") -eq "pre-existing") "must not clobber an existing PGPASSWORD"
        }

        Check "Restore-DatabasePassword clears PGPASSWORD when there was none" {
            [Environment]::SetEnvironmentVariable("PGPASSWORD", "leftover-secret")
            Restore-DatabasePassword -Previous $null
            Assert-True ($null -eq [Environment]::GetEnvironmentVariable("PGPASSWORD")) "PGPASSWORD must be removed"
        }

        Check "Restore-DatabasePassword puts back a pre-existing PGPASSWORD exactly" {
            [Environment]::SetEnvironmentVariable("PGPASSWORD", "leftover-secret")
            Restore-DatabasePassword -Previous "operator-value"
            Assert-True ([Environment]::GetEnvironmentVariable("PGPASSWORD") -eq "operator-value") "must restore the prior value, not clear it"
        }
    } finally {
        [Environment]::SetEnvironmentVariable("PGPASSWORD", $origPg)
    }
}
finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host ("backup-integrity tests: {0} passed, {1} failed" -f $script:passes, $script:failures)
if ($script:failures -gt 0) { exit 1 }
Write-Host "BACKUP INTEGRITY TESTS: PASS" -ForegroundColor Green
exit 0