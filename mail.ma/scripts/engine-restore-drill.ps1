#requires -Version 5.1
<#
    Tira Mail - engine archive restore drill (H-08).

    A backup that has never been restored is a hypothesis. This takes an archive
    produced by scripts/engine-backup.ps1 and proves, on a SCRATCH extraction
    directory (never the live engine store, and it never starts the engine):

      1. the archive decrypts (when sealed) and its manifest hash is intact;
      2. engine.tar matches the size and SHA-256 recorded in the manifest;
      3. every inventoried file extracts, matches its recorded size, and re-hashes
         to its recorded SHA-256 (unless -SkipFileHash);
      4. no extra/uninventoried file slipped into the tarball;
      5. the extracted data/ is a complete RocksDB store: CURRENT names a MANIFEST
         that exists, and IDENTITY/OPTIONS are present.

    This is a static integrity + structure proof. It does NOT open RocksDB with the
    engine and does NOT prove the engine boots and serves mail from the copy - that
    requires a live engine and is recorded as a limitation in CORRECTIONS.md.

    Usage:
      $env:TIRA_SEAL_PASSPHRASE = "..."   # only for sealed archives
      .\scripts\engine-restore-drill.ps1 -BackupDir .\backups\tira-engine-20261110-120000
      .\scripts\engine-restore-drill.ps1 -BackupDir ... -Keep   # keep extraction

    Exit code 0 only if every assertion passed.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $BackupDir,
    [string] $SealTool = ".\target\debug\tira-seal.exe",
    [switch] $SkipFileHash,
    [switch] $Keep
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "backup-common.ps1")

if (-not (Test-Path -LiteralPath $BackupDir -PathType Container)) {
    throw "Backup directory not found: $BackupDir"
}

$tar = Resolve-Tar
$backupFull = (Resolve-Path -LiteralPath $BackupDir).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("tira-engine-restore-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null
$success = $false

try {
    Write-Host "Tira engine restore drill" -ForegroundColor Cyan
    Write-Host ("  archive : {0}" -f $backupFull) -ForegroundColor Gray
    Write-Host ("  work    : {0}" -f $work) -ForegroundColor Gray

    # ---- 1. materialise plaintext ----------------------------------------
    $names = @("engine.tar", "manifest.json", "manifest.sha256")
    $sealedCount = @($names | Where-Object { Test-Path -LiteralPath (Join-Path $backupFull "$_.tira") }).Count
    if ($sealedCount -eq $names.Count) {
        if (-not $env:TIRA_SEAL_PASSPHRASE) { throw "Archive is sealed but TIRA_SEAL_PASSPHRASE is not set." }
        if (-not (Test-Path -LiteralPath $SealTool)) {
            throw "tira-seal not found at $SealTool. Build it with: cargo build -p tira-vault --bin tira-seal"
        }
        Write-Host "  decrypting sealed archive" -ForegroundColor Gray
        foreach ($name in $names) {
            $openResult = Invoke-Native -Exe $SealTool -Arguments @(
                "open", "--in", (Join-Path $backupFull "$name.tira"), "--out", (Join-Path $work $name), "--force")
            if ($openResult.ExitCode -ne 0) { throw "tira-seal open failed for $name`: $($openResult.Output)" }
        }
    } elseif ($sealedCount -gt 0) {
        throw "inconsistent archive: $sealedCount of $($names.Count) files are sealed; refusing a partial archive"
    } else {
        foreach ($name in $names) {
            $source = Join-Path $backupFull $name
            if (-not (Test-Path -LiteralPath $source)) { throw "archive is incomplete: missing $name" }
            Copy-Item -LiteralPath $source -Destination (Join-Path $work $name)
        }
    }

    # ---- 2. manifest integrity -------------------------------------------
    $manifestPath = Join-Path $work "manifest.json"
    $hashFile = Join-Path $work "manifest.sha256"
    $expectedManifestHash = ((Get-Content -LiteralPath $hashFile -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $actualManifestHash = Get-Sha256 $manifestPath
    Assert-True ($expectedManifestHash -eq $actualManifestHash) `
        "manifest.json hash mismatch: expected $expectedManifestHash, got $actualManifestHash"
    Write-Host "  manifest hash ok" -ForegroundColor Green

    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    Assert-True ($manifest.format -eq "tira-engine-backup/1") "unsupported manifest format: $($manifest.format)"
    Assert-True ($manifest.engine.stop_verified -eq $true) "manifest does not attest a verified stop window"
    Assert-True ($manifest.archive.name -eq "engine.tar") "unexpected archive name: $($manifest.archive.name)"

    # ---- 3. archive hash --------------------------------------------------
    $engineTar = Join-Path $work "engine.tar"
    $tarSize = [long] (Get-Item -LiteralPath $engineTar).Length
    Assert-True ($tarSize -eq [long] $manifest.archive.bytes) `
        "engine.tar is $tarSize bytes, manifest says $($manifest.archive.bytes)"
    $tarHash = Get-Sha256 $engineTar
    Assert-True ($tarHash -eq [string] $manifest.archive.sha256) `
        "engine.tar hash mismatch: expected $($manifest.archive.sha256), got $tarHash"
    Write-Host "  engine.tar hash ok" -ForegroundColor Green

    # ---- 4. extract + per-file verification ------------------------------
    $extract = Join-Path $work "engine"
    New-Item -ItemType Directory -Path $extract | Out-Null
    $extractResult = Invoke-Native -Exe $tar -Arguments @("-xf", $engineTar, "-C", $extract)
    if ($extractResult.ExitCode -ne 0) { throw "tar extraction failed: $($extractResult.Output)" }
    $extractFull = [IO.Path]::GetFullPath($extract).TrimEnd('\')

    $seen = New-Object 'System.Collections.Generic.HashSet[string]'
    $checked = 0
    foreach ($entry in $manifest.inventory.files) {
        $name = [string] $entry.name
        Assert-True ($name -match '^[A-Za-z0-9._][A-Za-z0-9._/-]*$') "unsafe inventory name: $name"
        Assert-True ($name -notmatch '(^|/)\.\.(/|$)') "inventory name contains '..': $name"
        $path = Join-Path $extract ($name -replace '/', '\')
        $full = [IO.Path]::GetFullPath($path)
        Assert-True ($full.StartsWith($extractFull + '\', [StringComparison]::OrdinalIgnoreCase)) `
            "inventory name escapes the extraction directory: $name"
        Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "missing archived file: $name"
        $size = [long] (Get-Item -LiteralPath $path).Length
        Assert-True ($size -eq [long] $entry.bytes) "$name is $size bytes, manifest says $($entry.bytes)"
        if (-not $SkipFileHash) {
            $hash = Get-Sha256 $path
            Assert-True ($hash -eq [string] $entry.sha256) "$name hash mismatch: expected $($entry.sha256), got $hash"
        }
        [void] $seen.Add($name)
        $checked++
    }
    Assert-True ($checked -eq [int] $manifest.inventory.count) `
        "verified $checked files, manifest inventory count is $($manifest.inventory.count)"

    # No uninventoried file may hide in the tarball.
    $extra = @()
    foreach ($f in @(Get-ChildItem -LiteralPath $extract -Recurse -File)) {
        $rel = $f.FullName.Substring($extractFull.Length).TrimStart('\', '/') -replace '\\', '/'
        if (-not $seen.Contains($rel)) { $extra += $rel }
    }
    Assert-True ($extra.Count -eq 0) "tarball holds $($extra.Count) uninventoried file(s): $(@($extra | Select-Object -First 5) -join ', ')"
    Write-Host ("  files ok ({0} verified, 0 extra, hashing {1})" -f $checked, $(if ($SkipFileHash) { "skipped" } else { "full SHA-256" })) -ForegroundColor Green

    # ---- 5. RocksDB structure --------------------------------------------
    $structure = Test-RocksDbStructure -DataDir (Join-Path $extract "data")
    Assert-True ($null -eq $structure) "extracted data/ is not a complete RocksDB store: $structure"
    Write-Host "  RocksDB structure ok (CURRENT -> MANIFEST, IDENTITY, OPTIONS)" -ForegroundColor Green

    $success = $true
    Write-Host ""
    Write-Host "ENGINE RESTORE DRILL PASSED" -ForegroundColor Green
    Write-Host ("  files verified : {0}" -f $checked) -ForegroundColor Gray
    Write-Host ("  bytes          : {0:N0}" -f [long] $manifest.inventory.total_bytes) -ForegroundColor Gray
    Write-Host ("  tls included   : {0}" -f [bool] $manifest.engine.tls_included) -ForegroundColor Gray
    Write-Host ""
    Write-Host "Next: restore onto different hardware and boot a real engine from it; this drill proves bytes, not service." -ForegroundColor Cyan
}
finally {
    if ($Keep) {
        Write-Host ("Kept work directory: {0}" -f $work) -ForegroundColor Yellow
    } elseif ($success) {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        Write-Warning "Drill failed; work directory kept for inspection: $work"
    }
}