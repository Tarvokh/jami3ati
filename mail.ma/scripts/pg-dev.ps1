<#
.SYNOPSIS
    Starts a throwaway PostgreSQL for Tira Mail tests.

.DESCRIPTION
    Uses a user-scope portable PostgreSQL (no installer, no service, no admin
    rights) listening on a non-default port so it cannot collide with a real
    installation. Data lives under C:\tools\pgsql\data.

    Why this exists: the storage tests in crates/tira-store/tests/persist.rs
    are skipped without TIRA_TEST_DATABASE_URL, and a skipped test still prints
    green. This script is what makes those tests actually run.

.EXAMPLE
    .\scripts\pg-dev.ps1
    .\scripts\pg-dev.ps1 -Stop
#>
param(
    [int]$Port = 55432,
    [string]$Db = "tira_test",
    [string]$PgRoot = "C:\tools\pgsql",
    [switch]$Stop,
    [switch]$Reset
)

$ErrorActionPreference = "Stop"
$bin = Join-Path $PgRoot "bin"
$data = Join-Path $PgRoot "data"
$log = Join-Path $PgRoot "server.log"

# PostgreSQL on Windows starts every backend with CreateProcess, and each one
# re-resolves its DLLs through PATH. If an earlier PATH entry ships an older
# msvcp140.dll/vcruntime140.dll (the Adoptium JRE 8 bin does), the backend dies
# with 0xC0000142 "STATUS_DLL_INIT_FAILED" the moment a client connects — the
# server looks healthy until the first connection. Sanitising PATH here is the
# fix, not a workaround: the postmaster only ever needs its own bin and the
# system directories.
$env:Path = "$bin;$env:SystemRoot\system32;$env:SystemRoot"

if (-not (Test-Path (Join-Path $bin "pg_ctl.exe"))) {
    throw "PostgreSQL not found at $PgRoot. Download the portable zip from https://www.enterprisedb.com/download-postgresql-binaries and extract so that $PgRoot\bin\pg_ctl.exe exists."
}

if ($Stop) {
    & (Join-Path $bin "pg_ctl.exe") -D $data stop -m fast
    exit $LASTEXITCODE
}

if ($Reset -and (Test-Path $data)) {
    Write-Host "Removing $data" -ForegroundColor Yellow
    & (Join-Path $bin "pg_ctl.exe") -D $data stop -m immediate 2>$null
    Remove-Item -Recurse -Force $data
}

if (-not (Test-Path (Join-Path $data "PG_VERSION"))) {
    Write-Host "Initialising cluster in $data" -ForegroundColor Cyan
    & (Join-Path $bin "initdb.exe") -D $data -U postgres --auth=trust --encoding=UTF8 --locale=C | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "initdb failed" }
}

& (Join-Path $bin "pg_ctl.exe") -D $data status *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Starting PostgreSQL on port $Port" -ForegroundColor Cyan
    # No `| Out-Null` here. Piping pg_ctl keeps the pipe handle open in the
    # daemonised postmaster, so the pipeline never completes and the script
    # hangs even though the server is already up.
    & (Join-Path $bin "pg_ctl.exe") -D $data -l $log -o "-p $Port -c listen_addresses=127.0.0.1" -w start
    if ($LASTEXITCODE -ne 0) { throw "pg_ctl start failed; see $log" }
}

# Wait for the server to accept connections instead of assuming it is ready.
$env:PGCONNECT_TIMEOUT = "5"
$ready = $false
for ($i = 0; $i -lt 30; $i++) {
    & (Join-Path $bin "pg_isready.exe") -h 127.0.0.1 -p $Port *> $null
    if ($LASTEXITCODE -eq 0) { $ready = $true; break }
    Start-Sleep -Milliseconds 300
}
if (-not $ready) { throw "PostgreSQL did not become ready; see $log" }

# pg_isready only proves the postmaster listens. It does not prove a backend
# can start, which is where the 0xC0000142 DLL failure hides. Force one.
$probe = & (Join-Path $bin "psql.exe") -h 127.0.0.1 -p $Port -U postgres -w -tAc "SELECT 1" 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "PostgreSQL is listening but cannot serve a query (backend failed to start): $probe"
}

$exists = & (Join-Path $bin "psql.exe") -h 127.0.0.1 -p $Port -U postgres -w -tAc "SELECT 1 FROM pg_database WHERE datname = '$Db'"
if ($LASTEXITCODE -ne 0) { throw "could not list databases" }
# psql prints nothing when the database does not exist, and PowerShell turns
# that into $null rather than "".
if ("$exists".Trim() -ne "1") {
    Write-Host "Creating database $Db" -ForegroundColor Cyan
    & (Join-Path $bin "createdb.exe") -h 127.0.0.1 -p $Port -U postgres $Db
    if ($LASTEXITCODE -ne 0) { throw "createdb failed" }
}

$url = "postgres://postgres@127.0.0.1:$Port/$Db"
Write-Host ""
Write-Host "PostgreSQL ready: $url" -ForegroundColor Green
Write-Host "Run the storage tests with:" -ForegroundColor Gray
Write-Host "  `$env:TIRA_TEST_DATABASE_URL = `"$url`"" -ForegroundColor Gray
Write-Host "  cargo test -p tira-store --test persist" -ForegroundColor Gray
