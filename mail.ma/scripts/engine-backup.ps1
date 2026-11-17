#requires -Version 5.1
<#
    Tira Mail - Stalwart engine backup (stop-window, H-08).

    Why a stop window
    -----------------
    Stalwart stores mail, accounts and its own configuration in an embedded
    RocksDB directory (config.json -> {"@type":"RocksDb","path":".../data"}).
    RocksDB is a live LSM tree: SST files, the WAL (.log), CURRENT and MANIFEST
    change during compaction. Copying it while the engine runs produces a
    silently inconsistent, unrestorable store - there is no verified online
    snapshot command in the version we run. The only safe copy is a QUIESCENT
    one: stop the engine, let RocksDB close and release its LOCK, then copy.

    What this script does
    ---------------------
      * VERIFIES the engine is stopped (fail-closed): no `stalwart` process, the
        management listener does not answer, and the RocksDB LOCK is not held.
        If any signal says "running", it refuses and changes nothing.
      * Copies config.json/config.toml and the whole data/ tree (read-only on the
        source) into a staging directory, optionally tls/ with -IncludeTls.
      * Records a per-file inventory (relative name, size, SHA-256), tars the
        staged tree to engine.tar, and pins manifest.json with manifest.sha256.
      * Optionally seals every artifact with tira-seal (TIRAS2,
        ChaCha20-Poly1305) so the archive is authenticated at rest, not just
        integrity-pinned.

    What this script does NOT do
    ----------------------------
      * It does NOT stop or start the engine. Killing a live mail server is the
        operator's / service manager's job, done gracefully so RocksDB flushes.
      * It does NOT include tls/ by default: key.pem is a private key. Re-provision
        TLS on restore, or pass -IncludeTls AND -Seal to archive it encrypted.
      * It is NOT an engine-native logical export. It is a cold physical copy; the
        drill proves the copy is complete and intact, not that the engine boots
        from it (that needs a live engine - see CORRECTIONS.md limitations).

    Operator stop-window procedure
    ------------------------------
      1. Announce a maintenance window; stopping Stalwart stops SMTP/IMAP/JMAP.
      2. Stop the engine the way you started it (service controller, console
         Ctrl+C, or process manager). Do NOT force-kill if you can avoid it, so
         RocksDB flushes the WAL and closes cleanly. Wait for the process to exit.
      3. Run this script. It re-checks "stopped" and refuses if the engine is up.
      4. Restart the engine; confirm the management URL answers and a test
         login/IMAP works before closing the window.
      5. Prove the archive restores: .\scripts\engine-restore-drill.ps1 -BackupDir <dir>
         (on a scratch copy, never the live data directory).

    Usage
    -----
      .\scripts\engine-backup.ps1 -OutDir D:\backups
      .\scripts\engine-backup.ps1 -EngineRunDir C:\tools\stalwart\run -OutDir D:\backups

      # Encrypted at rest (recommended; the store holds mail and credentials):
      $env:TIRA_SEAL_PASSPHRASE = "...long random passphrase..."
      .\scripts\engine-backup.ps1 -OutDir D:\backups -Seal

    Exit code 0 only if the engine was verified stopped, the copy is complete,
    and the manifest was written and read back.
#>
[CmdletBinding()]
param(
    [string] $EngineRunDir = "C:\tools\stalwart\run",
    [string] $DataDir,
    [string] $OutDir = ".\backups",
    [string] $ManagementUrl = "http://127.0.0.1:8080",
    [string] $ProcessName = "stalwart",
    [string] $SealTool = ".\target\debug\tira-seal.exe",
    [int]    $ChunkMiB = 4,
    [switch] $Seal,
    [switch] $IncludeTls
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "backup-common.ps1")

$target = $null
$stage = $null
$success = $false
$operationFailure = $null

# Only successful acquisition through New-BackupDirectory assigns an owned
# path. A collision or failed acquisition leaves it null: never delete another
# run's archive or staging directory. The live engine store is never cleaned.
try {
    if (-not (Test-Path -LiteralPath $EngineRunDir -PathType Container)) {
        throw "Engine run directory not found: $EngineRunDir"
    }

    # Derive the data directory from the engine's own config when not given.
    if (-not $DataDir) {
        $cfgPath = Join-Path $EngineRunDir "config.json"
        if (Test-Path -LiteralPath $cfgPath -PathType Leaf) {
            $cfg = Get-Content -LiteralPath $cfgPath -Raw | ConvertFrom-Json
            if ($cfg.path) {
                $p = ([string] $cfg.path) -replace '/', '\'
                # A relative store path is relative to the engine run dir, not cwd.
                if ([IO.Path]::IsPathRooted($p)) { $DataDir = $p }
                else { $DataDir = Join-Path $EngineRunDir $p }
            }
        }
        if (-not $DataDir) { $DataDir = Join-Path $EngineRunDir "data" }
    }
    if (-not (Test-Path -LiteralPath $DataDir -PathType Container)) {
        throw "Engine data directory not found: $DataDir"
    }

    # ---- fail-closed stop-window guard -----------------------------------
    if (-not (Test-EngineStopped -ProcessName $ProcessName -ManagementUrl $ManagementUrl -DataDir $DataDir)) {
        throw "Engine does not appear stopped (process '$ProcessName', listener $ManagementUrl, or RocksDB LOCK in $DataDir). Stop Stalwart gracefully and wait for it to exit before backing up: a live RocksDB copy is inconsistent. Refusing (fail-closed); nothing was copied."
    }

    # The source itself must be a complete RocksDB store before we archive it.
    $sourceStructure = Test-RocksDbStructure -DataDir $DataDir
    if ($sourceStructure) {
        throw "source data directory is not a complete RocksDB store: $sourceStructure"
    }

    if ($Seal) {
        if (-not (Test-Path -LiteralPath $SealTool)) {
            throw "tira-seal not found at $SealTool. Build it with: cargo build -p tira-vault --bin tira-seal"
        }
        if (-not $env:TIRA_SEAL_PASSPHRASE) {
            throw "TIRA_SEAL_PASSPHRASE is not set. Refusing to write a 'sealed' backup that is not sealed."
        }
    }

    $tar = Resolve-Tar

    $target = New-BackupDirectory -Parent $OutDir -Prefix 'tira-engine'
    $stage = New-BackupDirectory -Parent ([IO.Path]::GetTempPath()) -Prefix 'tira-engine-stage'

    Write-Host "Tira engine backup -> $target" -ForegroundColor Cyan
    Write-Host ("  run dir  : {0}" -f $EngineRunDir) -ForegroundColor Gray
    Write-Host ("  data dir : {0}" -f $DataDir) -ForegroundColor Gray
    Write-Host "  engine   : verified STOPPED (process + listener + LOCK)" -ForegroundColor Green

    # ---- stage a quiescent copy (source is read-only here) ---------------
    $configFiles = @()
    foreach ($name in @("config.json", "config.toml")) {
        $src = Join-Path $EngineRunDir $name
        if (Test-Path -LiteralPath $src -PathType Leaf) {
            Copy-Item -LiteralPath $src -Destination (Join-Path $stage $name) -Force
            $configFiles += $name
        }
    }

    $stageData = Join-Path $stage "data"
    New-Item -ItemType Directory -Path $stageData | Out-Null
    Get-ChildItem -LiteralPath $DataDir -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $stageData -Recurse -Force
    }

    if ($IncludeTls) {
        $tlsSrc = Join-Path $EngineRunDir "tls"
        if (Test-Path -LiteralPath $tlsSrc -PathType Container) {
            $stageTls = Join-Path $stage "tls"
            New-Item -ItemType Directory -Path $stageTls | Out-Null
            Get-ChildItem -LiteralPath $tlsSrc -Force | ForEach-Object {
                Copy-Item -LiteralPath $_.FullName -Destination $stageTls -Recurse -Force
            }
            Write-Warning "tls/ included: it holds a private key. Keep this archive sealed (-Seal) and access-controlled."
        } else {
            Write-Warning "-IncludeTls given but no tls/ directory at $tlsSrc"
        }
    }

    # ---- inventory + tar --------------------------------------------------
    $inventory = Get-FileInventory -Root $stage
    if ($inventory.Count -eq 0) { throw "staged copy is empty; refusing to archive nothing" }
    $stageStructure = Test-RocksDbStructure -DataDir $stageData
    if ($stageStructure) { throw "staged copy failed RocksDB structure check: $stageStructure" }

    $engineTar = Join-Path $target "engine.tar"
    Write-Host ("  tar {0} file(s), {1:N0} bytes" -f $inventory.Count, $inventory.TotalBytes) -ForegroundColor Gray
    $tarResult = Invoke-Native -Exe $tar -Arguments @("-cf", $engineTar, "-C", $stage, ".")
    if ($tarResult.ExitCode -ne 0) { throw "tar failed: $($tarResult.Output)" }
    if (-not (Test-Path -LiteralPath $engineTar)) { throw "tar reported success but $engineTar does not exist" }

    # ---- manifest ---------------------------------------------------------
    $manifest = [ordered]@{
        format     = "tira-engine-backup/1"
        created_at = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
        tool       = "scripts/engine-backup.ps1"
        engine     = [ordered]@{
            run_dir        = $EngineRunDir
            data_dir       = $DataDir
            store_type     = "RocksDb"
            management_url = $ManagementUrl
            process_name   = $ProcessName
            stop_verified  = $true
            tls_included   = [bool] $IncludeTls
            config_files   = $configFiles
        }
        inventory  = [ordered]@{
            count       = $inventory.Count
            total_bytes = $inventory.TotalBytes
            files       = $inventory.Files
        }
        archive    = [ordered]@{
            name   = "engine.tar"
            bytes  = [long] (Get-Item -LiteralPath $engineTar).Length
            sha256 = Get-Sha256 $engineTar
        }
        restore_hint = ".\scripts\engine-restore-drill.ps1 -BackupDir `"$target`" [-Keep]"
        sealed       = [bool] $Seal
    }
    if ($Seal) {
        $manifest.seal = [ordered]@{ format = "TIRAS2"; chunk_mib = $ChunkMiB; sealed_by = "tira-seal" }
    }

    $manifestJson = $manifest | ConvertTo-Json -Depth 8
    $manifestPath = Join-Path $target "manifest.json"
    Write-Utf8NoBom -Path $manifestPath -Text ($manifestJson + [Environment]::NewLine)
    $manifestHash = Get-Sha256 $manifestPath
    $pinPath = Join-Path $target "manifest.sha256"
    Write-Utf8NoBom -Path $pinPath -Text ("{0}  manifest.json`n" -f $manifestHash)

    # Read the pin back and re-hash the manifest from disk (same check the drill
    # runs), so a bad write cannot ship an unverifiable manifest.
    $pinReadBack = ((Get-Content -LiteralPath $pinPath -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $manifestReHash = Get-Sha256 $manifestPath
    Assert-True ($pinReadBack -eq $manifestReHash) `
        "manifest.sha256 read-back ($pinReadBack) does not pin manifest.json ($manifestReHash)"

    # ---- optional sealing -------------------------------------------------
    if ($Seal) {
        Write-Host "  sealing archive files" -ForegroundColor Gray
        $namesToSeal = @("engine.tar", "manifest.json", "manifest.sha256")
        $sealError = ""
        foreach ($name in $namesToSeal) {
            $plain = Join-Path $target $name
            $sealed = "$plain.tira"
            $sealResult = Invoke-Native -Exe $SealTool -Arguments @(
                "seal", "--in", $plain, "--out", $sealed, "--chunk-mib", "$ChunkMiB", "--force")
            if ($sealResult.ExitCode -ne 0) { $sealError = "tira-seal failed for ${name}: $($sealResult.Output)"; break }
        }
        if (-not $sealError) {
            foreach ($name in $namesToSeal) {
                $verifyResult = Invoke-Native -Exe $SealTool -Arguments @("verify", "--in", (Join-Path $target "$name.tira"))
                if ($verifyResult.ExitCode -ne 0) { $sealError = "tira-seal verify failed for ${name}: $($verifyResult.Output)"; break }
            }
        }
        if ($sealError) { throw "$sealError. The incomplete backup directory will be removed." }
        foreach ($name in $namesToSeal) { Remove-Item -LiteralPath (Join-Path $target $name) -Force }
    } else {
        Write-Warning "Archive is NOT encrypted or authenticated. It holds the engine's mail and credentials in clear text, and manifest.sha256 is an integrity pin only. Move it to encrypted storage or re-run with -Seal."
    }

    # Preserve a completed archive even if plaintext-stage cleanup later fails.
    # Do not announce success until the finally block has completed.
    $success = $true
    $archiveBytes = [long] ((Get-ChildItem -LiteralPath $target -File | Measure-Object -Property Length -Sum).Sum)
}
catch {
    $operationFailure = $_.Exception
    throw
}
finally {
    $cleanupFailures = New-Object 'System.Collections.Generic.List[System.Exception]'
    try {
        if ($stage -and (Test-Path -LiteralPath $stage)) {
            # Staging may contain plaintext credentials even for sealed output.
            # A failed cleanup must fail the command, not silently report success.
            Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction Stop
        }
    }
    catch { $cleanupFailures.Add($_.Exception) }
    # Attempt independent cleanup even after stage cleanup failed. Retain all
    # failures instead of allowing the last one to erase the original cause.
    try {
        if (-not $success -and $target -and (Test-Path -LiteralPath $target)) {
            Remove-Item -LiteralPath $target -Recurse -Force -ErrorAction Stop
            Write-Warning "Removed incomplete engine backup directory created by this run: $target"
        }
    }
    catch { $cleanupFailures.Add($_.Exception) }
    if ($cleanupFailures.Count -gt 0) {
        if ($operationFailure) { $cleanupFailures.Insert(0, $operationFailure) }
        $failureMessage = 'Engine backup cleanup failed: ' + (($cleanupFailures | ForEach-Object { $_.Message }) -join '; ')
        throw [System.AggregateException]::new($failureMessage,
            [System.Exception[]] $cleanupFailures.ToArray())
    }
}

Write-Host ""
Write-Host "Engine backup written" -ForegroundColor Green
Write-Host ("  path        : {0}" -f $target) -ForegroundColor Gray
Write-Host ("  size        : {0:N0} bytes" -f $archiveBytes) -ForegroundColor Gray
Write-Host ("  files       : {0} ({1:N0} bytes uncompressed)" -f $inventory.Count, $inventory.TotalBytes) -ForegroundColor Gray
Write-Host ("  sealed      : {0}" -f [bool] $Seal) -ForegroundColor Gray
Write-Host ("  tls included: {0}" -f [bool] $IncludeTls) -ForegroundColor Gray
Write-Host ""
Write-Host "Restart the engine, confirm it serves mail, then prove the archive:" -ForegroundColor Cyan
Write-Host ("  .\scripts\engine-restore-drill.ps1 -BackupDir `"{0}`"" -f $target) -ForegroundColor Gray
