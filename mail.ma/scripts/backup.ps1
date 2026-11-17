#requires -Version 5.1
<#
    Tira Mail - control-plane backup.

    What this archives:
      * PostgreSQL metadata (pg_dump custom format): tenants, domains,
        mailboxes, messages, recipients, API token hashes, audit rows.
      * The content-addressed blob store, the actual RFC 822 bytes, as one tar.

    What this deliberately does NOT archive:
      * Stalwart's own message store. The engine owns its RocksDB data directory
        and has no verified online-snapshot command in the version we run, so
        copying it live is not a backup. Use scripts/engine-backup.ps1 instead:
        it verifies a stop window, copies the quiescent store, and is drilled by
        scripts/engine-restore-drill.ps1 (H-08). The two archives are separate
        because their consistency models differ (online pg_dump vs cold copy).
      * Secrets: no SMTP password, no API token plaintext (only SHA-256 hashes
        exist in the database), no TLS private key.

    Consistency:
      The script records counts immediately before and immediately after the
      dump. If they differ, mail arrived during the backup and the manifest is
      marked "bounded": the dump is somewhere between the two counts. The
      restore drill accepts that range and still verifies every blob hash.
      For an exact point-in-time archive, run during a maintenance window; the
      manifest then says "quiescent".

    Integrity vs authenticity (M-21):
      The preflight re-hashes every referenced blob to its content address, so
      bit-rot or a same-size poison blob is caught BEFORE the archive is written,
      not just at restore. manifest.sha256 pins manifest.json; the restore drill
      verifies that pin, then every file hash, then every blob hash. That chain
      detects accidental corruption. It does NOT authenticate against a deliberate
      tamperer who can rewrite both manifest.json and manifest.sha256 - for that,
      seal the archive (-Seal): TIRAS2 ChaCha20-Poly1305 authenticates every
      file, so any tampering breaks the tag. Off-site copies, scheduling and
      retention are deliberately out of scope here (deferred; see
      CORRECTIONS.md).

    Usage:
      $env:TIRA_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
      $env:TIRA_BLOB_ROOT    = "C:\tira\data\blobs"

      .\scripts\backup.ps1 -OutDir D:\backups

      # Encrypted at rest (recommended):
      $env:TIRA_SEAL_PASSPHRASE = "...long random passphrase..."
      .\scripts\backup.ps1 -OutDir D:\backups -Seal

    Exit code 0 only if every step succeeded and the manifest was written.
#>
[CmdletBinding()]
param(
    [string] $DatabaseUrl = $env:TIRA_DATABASE_URL,
    [string] $BlobRoot = $(if ($env:TIRA_BLOB_ROOT) { $env:TIRA_BLOB_ROOT } else { ".\data\blobs" }),
    [string] $OutDir = ".\backups",
    [string] $PgBin = "C:\tools\pgsql\bin",
    [string] $SealTool = ".\target\debug\tira-seal.exe",
    [int]    $ChunkMiB = 4,
    [switch] $Seal,
    [switch] $AllowMissingBlobs
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "backup-common.ps1")

# A path is owned only after New-BackupDirectory returns successfully. Merely
# choosing a name (including one that already exists) never permits deletion.
$target = $null
$success = $false
$operationFailure = $null
$prevPgPassword = [Environment]::GetEnvironmentVariable("PGPASSWORD")
try {
if (-not $DatabaseUrl) {
    throw "DatabaseUrl is empty. Set TIRA_DATABASE_URL or pass -DatabaseUrl."
}

# H-09: move the password out of argv into PGPASSWORD; every psql/pg_dump call
# below uses the password-free URL. Get-TiraCounts strips defensively as well.
$SafeDatabaseUrl = ConvertTo-SafeDatabaseUrl $DatabaseUrl

$psql    = Join-Path $PgBin "psql.exe"
$pgDump  = Join-Path $PgBin "pg_dump.exe"
foreach ($tool in @($psql, $pgDump)) {
    if (-not (Test-Path -LiteralPath $tool)) {
        throw "PostgreSQL tool not found: $tool (set -PgBin)"
    }
}

$tar = Resolve-Tar

if ($Seal) {
    if (-not (Test-Path -LiteralPath $SealTool)) {
        throw "tira-seal not found at $SealTool. Build it with: cargo build -p tira-vault --bin tira-seal"
    }
    if (-not $env:TIRA_SEAL_PASSPHRASE) {
        throw "TIRA_SEAL_PASSPHRASE is not set. Refusing to write a 'sealed' backup that is not sealed."
    }
}

$blobRootFull = [System.IO.Path]::GetFullPath($BlobRoot)
$target = New-BackupDirectory -Parent $OutDir -Prefix 'tira'

Write-Host "Tira backup -> $target" -ForegroundColor Cyan
Write-Host ("  database : {0}" -f (Hide-DatabasePassword $DatabaseUrl)) -ForegroundColor Gray
Write-Host ("  blobs    : {0}" -f $blobRootFull) -ForegroundColor Gray

# ---- metadata snapshot ---------------------------------------------------

$versionResult = Invoke-Native -Exe $psql -Arguments @(
    "--dbname=$SafeDatabaseUrl",
    "--no-psqlrc",
    "--tuples-only",
    "--no-align",
    "--command=SHOW server_version"
)
if ($versionResult.ExitCode -ne 0) {
    throw "cannot query the database: $($versionResult.Output)"
}
$serverVersion = $versionResult.Output

$schemaResult = Invoke-Native -Exe $psql -Arguments @(
    "--dbname=$SafeDatabaseUrl",
    "--no-psqlrc",
    "--tuples-only",
    "--no-align",
    "--command=SELECT version FROM schema_version ORDER BY version"
)
if ($schemaResult.ExitCode -ne 0) {
    throw "cannot read schema_version: $($schemaResult.Output)"
}
$schemaVersions = @(
    $schemaResult.Output -split "`r?`n" |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ }
)

Write-Host "  counting rows before dump" -ForegroundColor Gray
$before = Get-TiraCounts -Psql $psql -DatabaseUrl $SafeDatabaseUrl

# ---- preflight: is every referenced blob on disk AND intact? ------------
# A dump that references a blob missing from the tar is not a backup, and a
# blob whose bytes no longer re-hash to their content address is poison. The
# restore drill would catch both, but failing here avoids shipping a broken
# archive to off-site storage in the first place (M-21).

Write-Host "  checking message blobs" -ForegroundColor Gray
$blobListResult = Invoke-Native -Exe $psql -Arguments @(
    "--dbname=$SafeDatabaseUrl",
    "--no-psqlrc",
    "--tuples-only",
    "--no-align",
    "--field-separator=|",
    "--command=SELECT blob_key, size_bytes FROM messages GROUP BY blob_key, size_bytes ORDER BY blob_key"
)
if ($blobListResult.ExitCode -ne 0) {
    throw "cannot list message blobs: $($blobListResult.Output)"
}

$blobRows = @($blobListResult.Output -split "`r?`n" | Where-Object { $_.Trim() })
$preflight = Get-BlobPreflightState -BlobRoot $blobRootFull -Rows $blobRows
$missingBlobs   = $preflight.Missing
$wrongSizeBlobs = $preflight.WrongSize
$corruptBlobs   = $preflight.Corrupt
$checkedBlobs   = $preflight.Checked

$blobState = "ok"
# Corrupt content (right size, wrong hash) is never archived, even with
# -AllowMissingBlobs: that flag covers absent content, not silent poison.
# Catching it here means a broken archive never leaves for off-site storage.
if ($corruptBlobs.Count -gt 0) {
    $sample = @($corruptBlobs | Select-Object -First 5) -join '; '
    throw "backup would archive CORRUPT blobs (content does not re-hash to its address): corrupt=$($corruptBlobs.Count). Examples: $sample. Restore the correct bytes or delete the orphan rows; -AllowMissingBlobs does not cover corruption."
}
if ($missingBlobs.Count -gt 0 -or $wrongSizeBlobs.Count -gt 0) {
    $blobState = "incomplete"
    $summary = "missing={0}, wrong_size={1}" -f $missingBlobs.Count, $wrongSizeBlobs.Count
    if (-not $AllowMissingBlobs) {
        $sample = @($missingBlobs + $wrongSizeBlobs | Select-Object -First 5) -join '; '
        throw "backup would be incomplete ($summary). Examples: $sample. Restore the blob files, delete the orphan rows, or pass -AllowMissingBlobs to archive metadata only."
    }
    Write-Warning "Archiving with incomplete blobs ($summary); the restore drill will fail on the missing content."
}
Write-Host ("  blobs ok ({0} distinct key/size pair(s), content re-hashed)" -f $checkedBlobs) -ForegroundColor $(if ($blobState -eq "ok") { "Green" } else { "Yellow" })

# ---- database dump -------------------------------------------------------

$dbDump = Join-Path $target "db.dump"
Write-Host "  pg_dump (custom format)" -ForegroundColor Gray
$dumpResult = Invoke-Native -Exe $pgDump -Arguments @(
    "--dbname=$SafeDatabaseUrl",
    "--format=custom",
    "--no-owner",
    "--no-privileges",
    "--file=$dbDump"
)
if ($dumpResult.ExitCode -ne 0) {
    throw "pg_dump failed: $($dumpResult.Output)"
}
if (-not (Test-Path -LiteralPath $dbDump)) {
    throw "pg_dump reported success but $dbDump does not exist"
}

# ---- blob archive --------------------------------------------------------

$blobsTar = Join-Path $target "blobs.tar"
$blobCount = 0
$blobBytes = 0L
if (Test-Path -LiteralPath $blobRootFull) {
    $blobFiles = @(
        Get-ChildItem -LiteralPath $blobRootFull -Recurse -File |
            Where-Object { $_.FullName -notmatch '[\\/]tmp[\\/]' }
    )
    $blobCount = $blobFiles.Count
    if ($blobCount -gt 0) {
        $blobBytes = [long](($blobFiles | Measure-Object -Property Length -Sum).Sum)
    }
    Write-Host ("  tar {0} blob file(s), {1:N0} bytes" -f $blobCount, $blobBytes) -ForegroundColor Gray
    $tarResult = Invoke-Native -Exe $tar -Arguments @("--exclude=tmp", "-cf", $blobsTar, "-C", $blobRootFull, ".")
} else {
    Write-Warning "Blob root does not exist: $blobRootFull (archiving an empty blob store)"
    $empty = Join-Path $target ".empty-blobs"
    New-Item -ItemType Directory -Path $empty | Out-Null
    $tarResult = Invoke-Native -Exe $tar -Arguments @("-cf", $blobsTar, "-C", $empty, ".")
    Remove-Item -LiteralPath $empty -Force
}
if ($tarResult.ExitCode -ne 0) {
    throw "tar failed: $($tarResult.Output)"
}

Write-Host "  counting rows after dump" -ForegroundColor Gray
$after = Get-TiraCounts -Psql $psql -DatabaseUrl $SafeDatabaseUrl

$countKeys = @(
    "tenants", "domains", "mailboxes", "messages",
    "recipients", "tokens", "message_bytes", "used_bytes"
)
$quiescent = $true
foreach ($key in $countKeys) {
    if ([long]$before.$key -ne [long]$after.$key) { $quiescent = $false }
}

# ---- manifest ------------------------------------------------------------

$manifest = [ordered]@{
    format     = "tira-backup/1"
    created_at = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    tool       = "scripts/backup.ps1"
    database   = [ordered]@{
        url_redacted     = Hide-DatabasePassword $DatabaseUrl
        server_version   = $serverVersion
        schema_versions  = $schemaVersions
        counts_before    = $before
        counts_after     = $after
        consistency      = $(if ($quiescent) { "quiescent" } else { "bounded" })
    }
    blobs      = [ordered]@{
        root    = $blobRootFull
        files   = $blobCount
        bytes   = $blobBytes
        checked = $checkedBlobs
        state   = $blobState
    }
    files      = @(
        [ordered]@{
            name   = "db.dump"
            bytes  = [long](Get-Item -LiteralPath $dbDump).Length
            sha256 = Get-Sha256 $dbDump
        },
        [ordered]@{
            name   = "blobs.tar"
            bytes  = [long](Get-Item -LiteralPath $blobsTar).Length
            sha256 = Get-Sha256 $blobsTar
        }
    )
    engine     = [ordered]@{
        included = $false
        note     = "Stalwart's RocksDB message store is NOT in this archive (it needs a stop window, not an online dump). Back it up separately with scripts/engine-backup.ps1 and drill it with scripts/engine-restore-drill.ps1, or chain both: restore-drill.ps1 -BackupDir <this> -EngineBackupDir <engine archive>. This archive restores the control plane and blob store only."
    }
    restore_hint = ".\scripts\restore-drill.ps1 -BackupDir `"$target`" [-Keep]"
    sealed       = [bool]$Seal
}
if ($Seal) {
    $manifest.seal = [ordered]@{
        format     = "TIRAS2"
        chunk_mib  = $ChunkMiB
        sealed_by  = "tira-seal"
    }
}

$manifestJson = $manifest | ConvertTo-Json -Depth 8
$manifestPath = Join-Path $target "manifest.json"
Write-Utf8NoBom -Path $manifestPath -Text ($manifestJson + [Environment]::NewLine)

$manifestHash = Get-Sha256 $manifestPath
$pinPath = Join-Path $target "manifest.sha256"
Write-Utf8NoBom -Path $pinPath -Text ("{0}  manifest.json`n" -f $manifestHash)

# Read the pin back and re-hash the manifest from disk: the archive must leave
# with a manifest.sha256 that actually pins manifest.json (the same check the
# restore drill runs), so a bad write cannot ship an unverifiable manifest.
$pinReadBack = ((Get-Content -LiteralPath $pinPath -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
$manifestReHash = Get-Sha256 $manifestPath
Assert-True ($pinReadBack -eq $manifestReHash) `
    "manifest.sha256 read-back ($pinReadBack) does not pin manifest.json ($manifestReHash)"

# ---- optional sealing ----------------------------------------------------

if ($Seal) {
    Write-Host "  sealing archive files" -ForegroundColor Gray
    $namesToSeal = @("db.dump", "blobs.tar", "manifest.json", "manifest.sha256")
    $sealError = ""

    # Seal every file first; delete no plaintext until every seal verifies.
    # A failure halfway through must not leave a directory that is partly
    # plaintext and partly ciphertext while claiming to be a sealed backup.
    foreach ($name in $namesToSeal) {
        $plain = Join-Path $target $name
        $sealed = "$plain.tira"
        $sealResult = Invoke-Native -Exe $SealTool -Arguments @(
            "seal", "--in", $plain, "--out", $sealed,
            "--chunk-mib", "$ChunkMiB", "--force"
        )
        if ($sealResult.ExitCode -ne 0) {
            $sealError = "tira-seal failed for ${name}: $($sealResult.Output)"
            break
        }
    }

    if (-not $sealError) {
        foreach ($name in $namesToSeal) {
            $sealed = Join-Path $target "$name.tira"
            $verifyResult = Invoke-Native -Exe $SealTool -Arguments @("verify", "--in", $sealed)
            if ($verifyResult.ExitCode -ne 0) {
                $sealError = "tira-seal verify failed for ${name}: $($verifyResult.Output)"
                break
            }
        }
    }

    if ($sealError) {
        throw "$sealError. The incomplete backup directory will be removed."
    }

    foreach ($name in $namesToSeal) {
        Remove-Item -LiteralPath (Join-Path $target $name) -Force
    }
} else {
    Write-Warning "Archive is NOT encrypted or authenticated. It holds every message body in clear text, and manifest.sha256 is an integrity pin only (a tamperer can rewrite it). Move it to encrypted storage or re-run with -Seal for authenticated at-rest integrity."
}

# ---- summary -------------------------------------------------------------

$archiveBytes = [long]((Get-ChildItem -LiteralPath $target -File | Measure-Object -Property Length -Sum).Sum)

# Report success only after cleanup and password restoration finish.
$success = $true
}
catch {
    $operationFailure = $_.Exception
    throw
}
finally {
    try {
        if (-not $success -and $target -and (Test-Path -LiteralPath $target)) {
            Remove-Item -LiteralPath $target -Recurse -Force -ErrorAction Stop
            Write-Warning "Removed incomplete backup directory created by this run: $target"
        }
    }
    catch {
        if ($operationFailure) {
            $failureMessage = 'Backup failed and cleanup also failed. Operation: {0}; cleanup: {1}' -f `
                $operationFailure.Message, $_.Exception.Message
            throw [System.AggregateException]::new($failureMessage,
                [System.Exception[]] @($operationFailure, $_.Exception))
        }
        throw
    }
    finally {
        # Cleanup (or even warning output) can throw. Secret restoration must
        # still run; do not put it after cleanup in the same block.
        Restore-DatabasePassword -Previous $prevPgPassword
    }
}

Write-Host ""
Write-Host "Backup written" -ForegroundColor Green
Write-Host ("  path        : {0}" -f $target) -ForegroundColor Gray
Write-Host ("  size        : {0:N0} bytes" -f $archiveBytes) -ForegroundColor Gray
Write-Host ("  sealed      : {0}" -f [bool]$Seal) -ForegroundColor Gray
Write-Host ("  consistency : {0}" -f $manifest.database.consistency) -ForegroundColor $(if ($quiescent) { "Green" } else { "Yellow" })
Write-Host ("  messages    : {0} ({1:N0} bytes)" -f $after.messages, [long]$after.message_bytes) -ForegroundColor Gray
Write-Host ("  blobs       : {0} file(s), {1:N0} bytes" -f $blobCount, $blobBytes) -ForegroundColor Gray
if (-not $quiescent) {
    Write-Host "  note        : mail arrived during the dump; counts are bounded, not exact" -ForegroundColor Yellow
}
Write-Host ""
Write-Host "Prove it restores before you trust it:" -ForegroundColor Cyan
Write-Host ("  .\scripts\restore-drill.ps1 -BackupDir `"{0}`"" -f $target) -ForegroundColor Gray

