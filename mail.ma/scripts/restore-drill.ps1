#requires -Version 5.1
<#
    Tira Mail - restore drill.

    A backup that has never been restored is a hypothesis, not a backup. This
    script takes an archive produced by scripts/backup.ps1 and proves, on a
    scratch database and a scratch blob directory:

      1. the archive decrypts (when sealed) and its manifest hash is intact;
      2. every archived file matches the size and SHA-256 in the manifest;
      3. pg_restore completes with --exit-on-error;
      4. row counts match the recorded range (exact for a quiescent backup);
      5. every message's blob exists, has the recorded size, and re-hashes to
         its content address;
      6. mailbox used_bytes equals the sum of its messages;
      7. message tenant ids agree with their mailbox, and token hashes are
         32 bytes (never plaintext).

    Scratch names are unique by default. A supplied name must start with
    "tira_restore_" and be absent: existing databases are never pre-dropped.
    Cleanup requires successful creation by this invocation and a matching
    recorded database OID. Failed drills and -Keep retain their scratch data.
    This is not protection against a privileged administrator replacing a
    database during the final OID-check/drop interval; use an isolated server.

    Permissions are reconstructed from the local, versioned restore-access.ps1
    contract, not donor cluster ACLs/passwords. Only the exact 0001..0009 schema
    is supported. tira_app must already be a safe NOLOGIN role that the restore
    connection can SET ROLE to. -CreateAppRole explicitly permits creating that
    missing cluster-wide role and granting it to the restore user; it remains
    after this drill. Existing roles are never changed. Custom donor grants and
    extra application roles are NOT restored. The restore user owns objects and
    must remain separate from tira_app. The scratch database denies PUBLIC
    access; a different application connection needs explicit provisioning.

    Use only a trusted archive on an isolated server: pg_restore executes SQL.
    A manifest hash is not authenticity and this drill is not a SQL sandbox.

    Pass -EngineBackupDir <dir> to ALSO drill a Stalwart engine archive (produced
    by scripts/engine-backup.ps1) in the same command; it chains the dedicated
    engine-restore-drill.ps1, and a failure there fails this drill too (H-08).

    Usage:
      $env:TIRA_SEAL_PASSPHRASE = "..."   # only for sealed archives
      .\scripts\restore-drill.ps1 -BackupDir .\backups\tira-20261106-120000

      # Keep both the scratch DB and extracted files for inspection:
      .\scripts\restore-drill.ps1 -BackupDir ... -Keep

      # Drill the control plane and the engine archive together:
      .\scripts\restore-drill.ps1 -BackupDir ... -EngineBackupDir .\backups\tira-engine-...

    Exit code 0 only if every assertion passed.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $BackupDir,
    [string] $PgBin = "C:\tools\pgsql\bin",
    [string] $DbHost = "127.0.0.1",
    [int]    $Port = 55432,
    [string] $DbUser = "postgres",
    [string] $ScratchDb = ('tira_restore_' + [guid]::NewGuid().ToString('N')),
    [string] $SealTool = ".\target\debug\tira-seal.exe",
    [string] $EngineBackupDir,
    [switch] $SkipBlobHash,
    [switch] $Keep,
    [switch] $CreateAppRole
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "backup-common.ps1")
. (Join-Path $PSScriptRoot "restore-access.ps1")

if (-not (Test-Path -LiteralPath $BackupDir -PathType Container)) {
    throw "Backup directory not found: $BackupDir"
}
# ASCII lowercase plus the 63-byte PostgreSQL identifier ceiling prevents
# quoted-name surprises and truncation collisions; the prefix is NOT ownership.
if ($ScratchDb -cnotmatch '\Atira_restore_[a-z0-9_]{1,50}\z') {
    throw "ScratchDb must start with tira_restore_, use lowercase ASCII letters/digits/underscores, and be at most 63 characters."
}

$psql       = Join-Path $PgBin "psql.exe"
$pgRestore  = Join-Path $PgBin "pg_restore.exe"
$createdb   = Join-Path $PgBin "createdb.exe"
$dropdb     = Join-Path $PgBin "dropdb.exe"
foreach ($tool in @($psql, $pgRestore, $createdb, $dropdb)) {
    if (-not (Test-Path -LiteralPath $tool)) {
        throw "PostgreSQL tool not found: $tool (set -PgBin)"
    }
}

$tar = Resolve-Tar

$backupFull = (Resolve-Path -LiteralPath $BackupDir).Path
$work = New-BackupDirectory -Parent $env:TEMP -Prefix 'tira-restore'

$scratchUrl = "postgres://$DbUser@$DbHost`:$Port/$ScratchDb"
$created = $false
$scratchOid = $null
$success = $false
$dbKept = $false

function Get-ScratchDatabaseOid {
    # ScratchDb has already passed the strict ASCII allowlist, so this literal
    # cannot contain SQL syntax. Query the maintenance DB, never the live mail DB.
    $result = Invoke-Native -Exe $psql -Arguments @(
        "-h", $DbHost, "-p", "$Port", "-U", $DbUser, "-w",
        "--dbname=postgres", "--no-psqlrc", "--tuples-only", "--no-align",
        "--set=ON_ERROR_STOP=1",
        "--command=SELECT oid FROM pg_database WHERE datname = '$ScratchDb'"
    )
    if ($result.ExitCode -ne 0) { throw "cannot check scratch database identity: $($result.Output)" }
    $oid = ([string]$result.Output).Trim()
    if ($oid -and $oid -cnotmatch '^[1-9][0-9]*$') { throw "unexpected scratch database identity response" }
    return $oid
}

function Drop-Scratch {
    if (-not $created -or -not $scratchOid) { throw "refusing cleanup without proven scratch database creation" }
    if ((Get-ScratchDatabaseOid) -ne $scratchOid) {
        throw "scratch database identity changed; refusing to drop $ScratchDb"
    }
    # Do not terminate other clients. An occupied scratch database is retained
    # and the drill fails rather than pretending cleanup succeeded.
    $result = Invoke-Native -Exe $dropdb -Arguments @(
        "-h", $DbHost, "-p", "$Port", "-U", $DbUser, "-w", $ScratchDb
    )
    if ($result.ExitCode -ne 0) {
        throw "scratch cleanup failed; database and work files retained: $($result.Output)"
    }
}

try {
    Write-Host "Tira restore drill" -ForegroundColor Cyan
    Write-Host ("  archive : {0}" -f $backupFull) -ForegroundColor Gray
    Write-Host ("  scratch : {0} on {1}:{2}" -f $ScratchDb, $DbHost, $Port) -ForegroundColor Gray
    Write-Host ("  work    : {0}" -f $work) -ForegroundColor Gray

    # Refuse existing names before decrypting or extracting anything. This is
    # an early diagnostic; createdb's exclusive CREATE is the authoritative
    # acquisition if another process races this check.
    if (Get-ScratchDatabaseOid) {
        throw "Scratch database already exists: $ScratchDb. Choose a new name; existing databases are never dropped."
    }

    # ---- 1. materialise plaintext ---------------------------------------

    $names = @("db.dump", "blobs.tar", "manifest.json", "manifest.sha256")
    $sealedCount = @(
        $names | Where-Object { Test-Path -LiteralPath (Join-Path $backupFull "$_.tira") }
    ).Count

    if ($sealedCount -eq $names.Count) {
        if (-not $env:TIRA_SEAL_PASSPHRASE) {
            throw "Archive is sealed but TIRA_SEAL_PASSPHRASE is not set."
        }
        if (-not (Test-Path -LiteralPath $SealTool)) {
            throw "tira-seal not found at $SealTool. Build it with: cargo build -p tira-vault --bin tira-seal"
        }
        Write-Host "  decrypting sealed archive" -ForegroundColor Gray
        foreach ($name in $names) {
            $source = Join-Path $backupFull "$name.tira"
            $openResult = Invoke-Native -Exe $SealTool -Arguments @(
                "open", "--in", $source, "--out", (Join-Path $work $name), "--force"
            )
            if ($openResult.ExitCode -ne 0) {
                throw "tira-seal open failed for $name`: $($openResult.Output)"
            }
        }
    } elseif ($sealedCount -gt 0) {
        throw "inconsistent archive: $sealedCount of $($names.Count) files are sealed; refusing a partial archive"
    } else {
        foreach ($name in $names) {
            $source = Join-Path $backupFull $name
            if (-not (Test-Path -LiteralPath $source)) {
                throw "archive is incomplete: missing $name"
            }
            Copy-Item -LiteralPath $source -Destination (Join-Path $work $name)
        }
    }

    # ---- 2. manifest integrity ------------------------------------------

    $manifestPath = Join-Path $work "manifest.json"
    $hashFile = Join-Path $work "manifest.sha256"
    $expectedManifestHash = ((Get-Content -LiteralPath $hashFile -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $actualManifestHash = Get-Sha256 $manifestPath
    Assert-True ($expectedManifestHash -eq $actualManifestHash) `
        "manifest.json hash mismatch: expected $expectedManifestHash, got $actualManifestHash"
    Write-Host "  manifest hash ok" -ForegroundColor Green

    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    Assert-True ($manifest.format -eq "tira-backup/1") `
        "unsupported manifest format: $($manifest.format)"

    foreach ($entry in $manifest.files) {
        $name = [string]$entry.name
        Assert-True ($name -match '^[A-Za-z0-9._-]+$') "refusing unsafe file name in manifest: $name"
        $path = Join-Path $work $name
        Assert-True (Test-Path -LiteralPath $path) "manifest lists $name but the archive does not contain it"
        $size = [long](Get-Item -LiteralPath $path).Length
        Assert-True ($size -eq [long]$entry.bytes) `
            "$name is $size bytes, manifest says $($entry.bytes)"
        $hash = Get-Sha256 $path
        Assert-True ($hash -eq [string]$entry.sha256) `
            "$name hash mismatch: expected $($entry.sha256), got $hash"
    }
    Write-Host "  archive file hashes ok" -ForegroundColor Green

    Assert-RestoreSchema -Versions @($manifest.database.schema_versions)
    Initialize-RestoreRole -Psql $psql -DbHost $DbHost -Port $Port -DbUser $DbUser -CreateAppRole:$CreateAppRole

    # ---- 3. restore into scratch ----------------------------------------

    $dbDump = Join-Path $work "db.dump"
    $blobsTar = Join-Path $work "blobs.tar"

    Write-Host "  creating a new scratch database" -ForegroundColor Gray
    $createResult = Invoke-Native -Exe $createdb -Arguments @(
        "-h", $DbHost, "-p", "$Port", "-U", $DbUser, "-w", "--template=template0", $ScratchDb
    )
    if ($createResult.ExitCode -ne 0) {
        throw "createdb failed for $ScratchDb`: $($createResult.Output)"
    }
    $created = $true
    $scratchOid = Get-ScratchDatabaseOid
    if (-not $scratchOid) { throw "cannot prove the newly created scratch database identity; refusing restore" }

    Protect-RestoreDatabase -Psql $psql -DatabaseUrl $scratchUrl -ScratchDb $ScratchDb
    Write-Host "  pg_restore" -ForegroundColor Gray
    $restoreResult = Invoke-Native -Exe $pgRestore -Arguments @(
        "-h", $DbHost, "-p", "$Port", "-U", $DbUser, "-w",
        "--dbname=$ScratchDb", "--no-owner", "--no-privileges",
        "--exit-on-error", "--single-transaction", $dbDump
    )
    if ($restoreResult.ExitCode -ne 0) {
        throw "pg_restore failed: $($restoreResult.Output)"
    }

    # ---- 4. counts -------------------------------------------------------

    $restored = Get-TiraCounts -Psql $psql -DatabaseUrl $scratchUrl
    $consistency = [string]$manifest.database.consistency
    $countKeys = @(
        "tenants", "domains", "mailboxes", "messages",
        "recipients", "tokens", "message_bytes", "used_bytes"
    )
    foreach ($key in $countKeys) {
        $actual = [long]$restored.$key
        $before = [long]$manifest.database.counts_before.$key
        $after = [long]$manifest.database.counts_after.$key
        if ($consistency -eq "quiescent") {
            Assert-True ($actual -eq $before -and $before -eq $after) `
                "$key is $actual after restore, manifest recorded exact $before (and $after after dump)"
        } else {
            Test-InRange -Name $key -Actual $actual -Before $before -After $after
        }
    }
    Write-Host ("  counts ok ({0} messages, {1:N0} bytes)" -f $restored.messages, [long]$restored.message_bytes) -ForegroundColor Green

    $schemaResult = Invoke-Native -Exe $psql -Arguments @(
        "--dbname=$scratchUrl", "--no-psqlrc", "--tuples-only", "--no-align",
        "--command=SELECT version FROM schema_version ORDER BY version"
    )
    if ($schemaResult.ExitCode -ne 0) { throw "cannot read restored schema_version: $($schemaResult.Output)" }
    $restoredSchema = @(
        $schemaResult.Output -split "`r?`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ }
    )
    $expectedSchema = @($manifest.database.schema_versions | ForEach-Object { "$_" })
    Assert-True (($restoredSchema -join ',') -eq ($expectedSchema -join ',')) `
        "restored migrations [$($restoredSchema -join ', ')] do not match manifest [$($expectedSchema -join ', ')]"
    Write-Host ("  schema ok ({0} migration(s))" -f $restoredSchema.Count) -ForegroundColor Green
    Restore-TiraAccess -Psql $psql -DatabaseUrl $scratchUrl
    Write-Host "  access ok (owner-separated grants, private definer functions, RLS)" -ForegroundColor Green

    # ---- 5. structural checks -------------------------------------------

    $usageQuery = @'
SELECT count(*) FROM (
    SELECT mb.id, mb.used_bytes, COALESCE(sum(msg.size_bytes), 0) AS actual
      FROM mailboxes mb
      LEFT JOIN messages msg ON msg.mailbox_id = mb.id
     GROUP BY mb.id, mb.used_bytes
) s WHERE s.used_bytes <> s.actual
'@
    $usageResult = Invoke-Native -Exe $psql -Arguments @(
        "--dbname=$scratchUrl", "--no-psqlrc", "--tuples-only", "--no-align",
        "--command=$usageQuery"
    )
    if ($usageResult.ExitCode -ne 0) { throw "used_bytes check failed: $($usageResult.Output)" }
    $badUsage = $usageResult.Output
    Assert-True ([long]$badUsage -eq 0) "$badUsage mailbox(es) have used_bytes that disagree with their messages"

    $tenantQuery = @'
SELECT count(*) FROM messages msg
  JOIN mailboxes mb ON mb.id = msg.mailbox_id
 WHERE msg.tenant_id <> mb.tenant_id
'@
    $tenantResult = Invoke-Native -Exe $psql -Arguments @(
        "--dbname=$scratchUrl", "--no-psqlrc", "--tuples-only", "--no-align",
        "--command=$tenantQuery"
    )
    if ($tenantResult.ExitCode -ne 0) { throw "tenant consistency check failed: $($tenantResult.Output)" }
    $badTenant = $tenantResult.Output
    Assert-True ([long]$badTenant -eq 0) "$badTenant message(s) point at a mailbox in another tenant"

    $tokenResult = Invoke-Native -Exe $psql -Arguments @(
        "--dbname=$scratchUrl", "--no-psqlrc", "--tuples-only", "--no-align",
        "--command=SELECT count(*) FROM api_tokens WHERE octet_length(token_hash) <> 32"
    )
    if ($tokenResult.ExitCode -ne 0) { throw "token hash check failed: $($tokenResult.Output)" }
    $badTokens = $tokenResult.Output
    Assert-True ([long]$badTokens -eq 0) "$badTokens token(s) do not store a 32-byte SHA-256 hash"
    Write-Host "  structural checks ok" -ForegroundColor Green

    # ---- 6. blob verification -------------------------------------------

    $blobWork = Join-Path $work "blobs"
    New-Item -ItemType Directory -Path $blobWork | Out-Null
    $extractResult = Invoke-Native -Exe $tar -Arguments @("-xf", $blobsTar, "-C", $blobWork)
    if ($extractResult.ExitCode -ne 0) {
        throw "tar extraction failed: $($extractResult.Output)"
    }

    $rowsResult = Invoke-Native -Exe $psql -Arguments @(
        "--dbname=$scratchUrl", "--no-psqlrc", "--tuples-only", "--no-align",
        "--field-separator=|",
        "--command=SELECT blob_key, sha256, size_bytes FROM messages"
    )
    if ($rowsResult.ExitCode -ne 0) {
        throw "cannot list message blobs: $($rowsResult.Output)"
    }
    $rows = @(
        $rowsResult.Output -split "`r?`n" | Where-Object { $_.Trim() }
    )

    $seen = New-Object 'System.Collections.Generic.HashSet[string]'
    $uniqueBlobs = 0
    $uniqueBytes = 0L
    $checkedRows = 0

    foreach ($row in $rows) {
        $line = "$row".Trim()
        if (-not $line) { continue }
        $parts = $line.Split('|')
        if ($parts.Count -ne 3) { throw "unexpected blob row: $line" }

        $key = $parts[0]
        $expectedSha = $parts[1].ToLowerInvariant()
        $expectedSize = [long]$parts[2]
        $checkedRows++

        Assert-True ($key -match '^[0-9a-f]{2}/[0-9a-f]{2}/[0-9a-f]{64}$') "invalid blob key: $key"
        Assert-True ($key.Split('/')[-1] -eq $expectedSha) `
            "blob key $key does not match recorded sha256 $expectedSha"

        if (-not $seen.Add($key)) { continue }
        $uniqueBlobs++
        $uniqueBytes += $expectedSize

        $path = Join-Path $blobWork ($key -replace '/', '\')
        Assert-True (Test-Path -LiteralPath $path) "missing blob in archive: $key"
        $actualSize = [long](Get-Item -LiteralPath $path).Length
        Assert-True ($actualSize -eq $expectedSize) `
            "blob $key is $actualSize bytes, database says $expectedSize"
        if (-not $SkipBlobHash) {
            $actualSha = Get-Sha256 $path
            Assert-True ($actualSha -eq $expectedSha) `
                "blob $key hash mismatch: expected $expectedSha, got $actualSha"
        }
    }

    $extracted = @(Get-ChildItem -LiteralPath $blobWork -Recurse -File)
    $orphans = $extracted.Count - $uniqueBlobs
    Write-Host ("  blobs ok ({0} message row(s), {1} unique blob(s), {2:N0} bytes)" -f `
            $checkedRows, $uniqueBlobs, $uniqueBytes) -ForegroundColor Green
    Write-Host ("  extracted files : {0} ({1} unreferenced, harmless)" -f $extracted.Count, $orphans) -ForegroundColor Gray

    # ---- 7. optional engine archive (H-08) -------------------------------
    # The control plane and the engine are separate archives with different
    # consistency models (pg_dump can be bounded/online; RocksDB needs a stop
    # window). When -EngineBackupDir is given, drill both in one command by
    # chaining the dedicated engine drill; a failure there fails this drill too.
    if ($EngineBackupDir) {
        Write-Host "  chaining engine restore drill" -ForegroundColor Gray
        $engineDrill = Join-Path $PSScriptRoot "engine-restore-drill.ps1"
        if (-not (Test-Path -LiteralPath $engineDrill)) { throw "engine drill not found: $engineDrill" }
        $psExe = Join-Path $PSHOME "powershell.exe"
        $engineArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $engineDrill,
            "-BackupDir", $EngineBackupDir, "-SealTool", $SealTool)
        if ($Keep) { $engineArgs += "-Keep" }
        $engineResult = Invoke-Native -Exe $psExe -Arguments $engineArgs
        if ($engineResult.ExitCode -ne 0) { throw "engine restore drill failed: $($engineResult.Output)" }
        Write-Host "  engine archive ok" -ForegroundColor Green
    }

    $success = $true
    # The success banner is emitted only AFTER final cleanup has succeeded.
}
finally {
    if ($created) {
        if ($Keep -or -not $success) {
            $dbKept = $true
        } else {
            Drop-Scratch
        }
    }

    if ($Keep) {
        Write-Host ("Kept work directory: {0}" -f $work) -ForegroundColor Yellow
        if ($dbKept) {
            Write-Host ("Kept scratch database: {0}" -f $ScratchDb) -ForegroundColor Yellow
        }
    } elseif ($success) {
        Remove-Item -LiteralPath $work -Recurse -Force
    } else {
        Write-Warning "Drill failed; work directory kept for inspection: $work"
        if ($dbKept) {
            Write-Warning "Scratch database kept for inspection: $ScratchDb"
            Write-Warning "Inspect the retained database and confirm its identity before any manual cleanup."
        }
    }
}

Write-Host ""
Write-Host "RESTORE DRILL PASSED" -ForegroundColor Green
Write-Host ("  messages verified : {0}" -f $checkedRows) -ForegroundColor Gray
Write-Host ("  blobs verified    : {0}" -f $uniqueBlobs) -ForegroundColor Gray
Write-Host ("  consistency       : {0}" -f $consistency) -ForegroundColor Gray
Write-Host ("  blob hashing      : {0}" -f $(if ($SkipBlobHash) { "skipped" } else { "full SHA-256" })) -ForegroundColor Gray
Write-Host ""
Write-Host "Next: run the same drill against a copy on different hardware, not just this machine." -ForegroundColor Cyan
