#requires -Version 5.1
<#
    Shared helpers for Tira Mail backup and restore drills.

    Dot-source this file; it defines functions only and has no side effects.
    Keeping the count query in one place matters: if the backup manifest and the
    restore drill asked different questions, a drill could pass while the
    archive was incomplete.
#>

Set-StrictMode -Version Latest

# Allocates a fresh directory. Callers assign the returned path only after the
# exclusive New-Item succeeds; a failed acquisition must never authorize cleanup
# of a pre-existing directory. The random suffix avoids same-second collisions.
# Parent directories may already exist; the owned leaf must never use -Force.
function New-BackupDirectory {
    param(
        [Parameter(Mandatory)] [string] $Parent,
        [Parameter(Mandatory)] [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9_-]*$')] [string] $Prefix
    )
    $parentFull = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Parent)
    New-Item -ItemType Directory -Force -Path $parentFull -ErrorAction Stop | Out-Null
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
    $candidate = Join-Path $parentFull ('{0}-{1}-{2}' -f $Prefix, $stamp, [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $candidate -ErrorAction Stop | Out-Null
    return $candidate
}

function Get-Sha256 {
    param([Parameter(Mandatory)] [string] $Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# Verifies a content-addressed blob (M-21/M-09). The store names each blob by
# the SHA-256 of its bytes: key = xx/yy/<64-hex digest>. A size check cannot
# detect bit-rot or a same-length pre-planted poison blob; re-hashing the
# content to its address is the authoritative integrity test. Returns $true
# only when the file exists and its SHA-256 equals the digest in its key.
function Test-ContentAddress {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $Key
    )
    if ($Key -notmatch '^[0-9a-f]{2}/[0-9a-f]{2}/[0-9a-f]{64}$') {
        throw "invalid content-addressed key: $Key"
    }
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $expected = $Key.Split('/')[-1].ToLowerInvariant()
    return ((Get-Sha256 $Path) -eq $expected)
}

# Preflight integrity of every database-referenced blob, before anything is
# archived (M-21). Each row is "blob_key|size_bytes" as psql emits with
# --field-separator=|. A blob falls into exactly one category:
#   missing    - referenced but not on disk
#   wrong_size - on disk, length disagrees with the database
#   corrupt    - correct length, but content does not re-hash to its address
# 'checked' counts blobs found on disk (examined), pass or fail. Keeping this
# in one place lets backup.ps1 and a standalone test ask the same question; a
# preflight that only checked existence/size would ship corruption off-site.
function Get-BlobPreflightState {
    param(
        [Parameter(Mandatory)] [string] $BlobRoot,
        [Parameter(Mandatory)] [AllowEmptyCollection()] [string[]] $Rows
    )
    $missing   = New-Object 'System.Collections.Generic.List[string]'
    $wrongSize = New-Object 'System.Collections.Generic.List[string]'
    $corrupt   = New-Object 'System.Collections.Generic.List[string]'
    $checked = 0
    foreach ($line in $Rows) {
        if (-not "$line".Trim()) { continue }
        $parts = "$line".Split('|')
        if ($parts.Count -ne 2) { throw "unexpected blob row: $line" }
        $key = $parts[0]
        if ($key -notmatch '^[0-9a-f]{2}/[0-9a-f]{2}/[0-9a-f]{64}$') {
            throw "invalid blob key in database: $key"
        }
        $path = Join-Path $BlobRoot ($key -replace '/', '\')
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            $missing.Add($key)
            continue
        }
        $checked++
        $actualSize = [long](Get-Item -LiteralPath $path).Length
        if ($actualSize -ne [long]$parts[1]) {
            $wrongSize.Add(("{0} (on disk {1}, database {2})" -f $key, $actualSize, $parts[1]))
        } elseif (-not (Test-ContentAddress -Path $path -Key $key)) {
            $corrupt.Add(("{0} (content does not re-hash to its address)" -f $key))
        }
    }
    return [pscustomobject]@{
        Missing   = $missing
        WrongSize = $wrongSize
        Corrupt   = $corrupt
        Checked   = $checked
    }
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $Text
    )
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

# Runs a native tool without letting its stderr terminate the script.
#
# Windows PowerShell turns redirected native stderr into ErrorRecords, and
# $ErrorActionPreference = "Stop" then treats them as fatal. That is wrong for
# pg_dump/psql/dropdb, which write ordinary notices to stderr and still exit 0.
# The caller checks ExitCode instead.
function Invoke-Native {
    param(
        [Parameter(Mandatory)] [string] $Exe,
        [Parameter(Mandatory)] [string[]] $Arguments
    )
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = & $Exe @Arguments 2>&1
        $code = $LASTEXITCODE
        $text = (@($output) | ForEach-Object { "$_" }) -join [Environment]::NewLine
        return [pscustomobject]@{
            ExitCode = [int]$code
            Output   = $text.Trim()
        }
    }
    finally {
        $ErrorActionPreference = $previous
    }
}

# Resolves a tar that understands native Windows paths. Windows 10 1803+ ships
# bsdtar at %SystemRoot%\System32\tar.exe; prefer it explicitly. A GNU tar earlier
# in PATH (Git-for-Windows, MSYS2) misreads "C:\..." as a remote host:file spec
# and dies with "Cannot connect to C: resolve failed", so `Get-Command tar.exe`
# alone is not safe: it returns whichever tar wins PATH, which on a developer box
# is often GNU tar. Falls back to PATH only when System32 has no tar at all.
function Resolve-Tar {
    if ($env:SystemRoot) {
        $systemTar = Join-Path $env:SystemRoot "System32\tar.exe"
        if (Test-Path -LiteralPath $systemTar -PathType Leaf) {
            return $systemTar
        }
    }
    $cmd = Get-Command tar.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "tar.exe not found. Windows 10 1803+ ships bsdtar at %SystemRoot%\System32\tar.exe; install it, or copy blobs another way."
}

# Removes a password from a connection URL before it is written to a manifest.
# Postgres URLs may also carry credentials in the query string; those are simply
# dropped because a backup manifest is not the place for them.
function Hide-DatabasePassword {
    param([Parameter(Mandatory)] [string] $Url)
    $redacted = $Url -replace '://([^:/@]+):[^@/]*@', '://$1:***@'
    if ($redacted.Contains('?')) {
        $redacted = $redacted.Substring(0, $redacted.IndexOf('?'))
    }
    if ($redacted.Contains('#')) {
        $redacted = $redacted.Substring(0, $redacted.IndexOf('#'))
    }
    return $redacted
}

# libpq reads the password from PGPASSWORD (or .pgpass), never from argv.
# Passing `--dbname=postgres://user:pass@host/db` exposes the secret to every
# local user via the process command line (ps, Task Manager, CI logs). This
# moves the password into PGPASSWORD and returns a password-free URL that is
# safe to put in argv. Idempotent: a URL with no password is returned unchanged
# and PGPASSWORD is left as-is, so callers that already rely on PGPASSWORD or
# .pgpass (e.g. restore-drill.ps1) keep working. The password is percent-decoded
# because libpq expects the URI form decoded when it comes from the environment.
function ConvertTo-SafeDatabaseUrl {
    param([Parameter(Mandatory)] [string] $Url)
    if ($Url -match '://([^:/@]+):([^@/]*)@') {
        $env:PGPASSWORD = [System.Uri]::UnescapeDataString($Matches[2])
        return ($Url -replace '://([^:/@]+):[^@/]*@', '://$1@')
    }
    return $Url
}

# Restores PGPASSWORD to the value captured BEFORE ConvertTo-SafeDatabaseUrl ran
# (or removes it entirely when there was none). ConvertTo-SafeDatabaseUrl leaves
# the secret in the process environment so successive psql/pg_dump calls work;
# that is fine mid-operation but the secret must not outlive it. Callers capture
# the prior value once at startup and call this in a trap/finally AND at normal
# exit, so the database password never leaks into later child processes or lingers
# in the operator's session. Non-destructive: a pre-existing PGPASSWORD (e.g. set
# by the operator or restore-drill) is put back exactly as it was.
function Restore-DatabasePassword {
    param([AllowNull()][string] $Previous)
    [Environment]::SetEnvironmentVariable("PGPASSWORD", $Previous)
}

# Snapshot of the control-plane tables. Values are cast to text so PowerShell's
# JSON parser never rounds a byte count through a double.
function Get-TiraCounts {
    param(
        [Parameter(Mandatory)] [string] $Psql,
        [Parameter(Mandatory)] [string] $DatabaseUrl
    )

    $query = @'
SELECT json_build_object(
    'tenants',       (SELECT count(*) FROM tenants)::text,
    'domains',       (SELECT count(*) FROM domains)::text,
    'mailboxes',     (SELECT count(*) FROM mailboxes)::text,
    'messages',      (SELECT count(*) FROM messages)::text,
    'recipients',    (SELECT count(*) FROM message_recipients)::text,
    'tokens',        (SELECT count(*) FROM api_tokens)::text,
    'message_bytes', (SELECT COALESCE(sum(size_bytes), 0) FROM messages)::text,
    'used_bytes',    (SELECT COALESCE(sum(used_bytes), 0) FROM mailboxes)::text
)
'@

    $safeUrl = ConvertTo-SafeDatabaseUrl $DatabaseUrl
    $raw = Invoke-Native -Exe $Psql -Arguments @(
        "--dbname=$safeUrl",
        "--no-psqlrc",
        "--tuples-only",
        "--no-align",
        "--command=$query"
    )
    if ($raw.ExitCode -ne 0) {
        throw "count query failed: $($raw.Output)"
    }

    $text = $raw.Output
    if (-not $text) {
        throw "count query returned nothing"
    }
    return $text | ConvertFrom-Json
}

# Both scripts must agree on what "between two counts" means.
function Test-InRange {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [long] $Actual,
        [Parameter(Mandatory)] [long] $Before,
        [Parameter(Mandatory)] [long] $After
    )
    $low = [Math]::Min($Before, $After)
    $high = [Math]::Max($Before, $After)
    if ($Actual -lt $low -or $Actual -gt $high) {
        throw "$Name is $Actual, outside the recorded range [$low, $high]"
    }
}

function Assert-True {
    param(
        [Parameter(Mandatory)] [bool] $Condition,
        [Parameter(Mandatory)] [string] $Message
    )
    if (-not $Condition) { throw $Message }
}

# Probes a TCP management listener (H-08). Reachable => the engine is up. A short
# connect timeout keeps a firewalled/dropped port from hanging the backup. Any
# parse/connect error is treated as "not reachable" ($false); the caller combines
# this with the process and LOCK signals, so a bad URL alone cannot prove "up".
function Test-ManagementListener {
    param([Parameter(Mandatory)] [string] $Url, [int] $TimeoutMs = 1500)
    $uri = $null
    try { $uri = [Uri] $Url } catch { return $false }
    if (-not $uri.IsAbsoluteUri) { return $false }
    $tcp = New-Object System.Net.Sockets.TcpClient
    try {
        $iar = $tcp.BeginConnect($uri.Host, $uri.Port, $null, $null)
        if ($iar.AsyncWaitHandle.WaitOne($TimeoutMs, $false)) {
            try { $tcp.EndConnect($iar); return $true } catch { return $false }
        }
        return $false
    } catch {
        return $false
    } finally {
        $tcp.Close()
    }
}

# Fail-closed stop-window guard (H-08). A RocksDB store must never be copied while
# the engine holds it: SST/WAL/MANIFEST can be mid-compaction, so a live copy is
# silently inconsistent and unrestorable. Returns $true ONLY when all three
# signals agree the engine is down: no server process, management listener
# unreachable, and the RocksDB LOCK file not exclusively held. Any doubt =>
# $false, and the backup refuses. The LOCK probe is the authoritative signal for
# RocksDB; process name and URL are belt-and-suspenders for a renamed binary or a
# service wrapper. This never stops the engine: stopping is the operator's job.
function Test-EngineStopped {
    param(
        [string] $ProcessName = "stalwart",
        [string] $ManagementUrl = "http://127.0.0.1:8080",
        [string] $DataDir
    )
    if (Get-Process -Name $ProcessName -ErrorAction SilentlyContinue) { return $false }
    if (Test-ManagementListener -Url $ManagementUrl) { return $false }
    if ($DataDir) {
        $lock = Join-Path $DataDir "LOCK"
        if (Test-Path -LiteralPath $lock -PathType Leaf) {
            try {
                $fs = [System.IO.File]::Open(
                    $lock,
                    [System.IO.FileMode]::Open,
                    [System.IO.FileAccess]::ReadWrite,
                    [System.IO.FileShare]::None)
                $fs.Close()
            } catch {
                return $false   # LOCK held (or unreadable) => assume running
            }
        }
    }
    return $true
}

# Inventory of every file under $Root, as relative forward-slash names with size
# and SHA-256 (H-08). The engine archive records this so the drill can re-hash
# each extracted file individually, not just the tarball: a tar whose own hash
# matches could still hold a truncated member if it was built from a bad copy.
function Get-FileInventory {
    param([Parameter(Mandatory)] [string] $Root)
    $rootFull = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\', '/')
    $files = @(Get-ChildItem -LiteralPath $rootFull -Recurse -File | Sort-Object FullName)
    $inv = New-Object 'System.Collections.Generic.List[object]'
    $total = 0L
    foreach ($f in $files) {
        $rel = $f.FullName.Substring($rootFull.Length).TrimStart('\', '/') -replace '\\', '/'
        $len = [long] $f.Length
        $inv.Add([pscustomobject]@{ name = $rel; bytes = $len; sha256 = (Get-Sha256 $f.FullName) })
        $total += $len
    }
    return [pscustomobject]@{ Files = $inv; TotalBytes = $total; Count = $files.Count }
}

# Static structural check of a copied RocksDB directory (H-08). Returns $null when
# the directory looks like a complete RocksDB store, or a reason string otherwise.
# This is NOT an engine open (it never runs stalwart or mutates the copy): it
# proves CURRENT names a MANIFEST that actually exists and that IDENTITY/OPTIONS
# are present, so an interrupted or partial copy is rejected before it is trusted.
function Test-RocksDbStructure {
    param([Parameter(Mandatory)] [string] $DataDir)
    $current = Join-Path $DataDir "CURRENT"
    if (-not (Test-Path -LiteralPath $current -PathType Leaf)) { return "missing CURRENT" }
    if (-not (Test-Path -LiteralPath (Join-Path $DataDir "IDENTITY") -PathType Leaf)) { return "missing IDENTITY" }
    $manifestName = (Get-Content -LiteralPath $current -Raw).Trim()
    if ($manifestName -notmatch '^MANIFEST-[0-9]+$') { return "CURRENT does not name a MANIFEST: '$manifestName'" }
    if (-not (Test-Path -LiteralPath (Join-Path $DataDir $manifestName) -PathType Leaf)) {
        return "CURRENT references missing $manifestName"
    }
    $options = @(Get-ChildItem -LiteralPath $DataDir -Filter "OPTIONS-*" -File)
    if ($options.Count -eq 0) { return "no OPTIONS-* file" }
    return $null
}
