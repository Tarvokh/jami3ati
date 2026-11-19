<#
.SYNOPSIS
    Runs Tira Mail locally for real people to sign up and test.

.DESCRIPTION
    One command, no fixtures:

      1. starts the portable PostgreSQL on a non-default port,
      2. creates the `tira_dev` database if it does not exist,
      3. builds and starts `tira-api` bound to loopback with dev mode on,
      4. creates the single dev tenant + domain (`tira.test`) once,
      5. prints the signup URL.

    Why a separate database: `tira_test` is written by the integration tests and
    holds thousands of fixture tenants. Serving people from it is how the demo
    ended up showing fake universities. `tira_dev` starts empty, and the API
    refuses to start from a `*_test` database unless TIRA_ALLOW_TEST_DB=1.

    Why an in-process SMTP loop: the compose path submits to a relay. Pointing
    that relay at the bundled `tira-smtp` listener on loopback turns
    "send to yourself" into a real round trip through SMTP, storage and RLS,
    with no third-party service and no placeholder message.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\dev-local.ps1

.EXAMPLE
    .\scripts\dev-local.ps1 -Port 8090 -Domain dev.tira.test -NoBuild

.NOTES
    Stop with Ctrl+C. The script stops the API process it started; PostgreSQL
    keeps running (use .\scripts\pg-dev.ps1 -Stop).
#>
param(
    [int]$Port = 8080,
    [int]$DbPort = 55432,
    [string]$Db = "tira_dev",
    [string]$Domain = "tira.test",
    [int]$SmtpPort = 2525,
    [string]$PgRoot = "C:\tools\pgsql",
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$pgBin = Join-Path $PgRoot "bin"
$psql = Join-Path $pgBin "psql.exe"

if ($Db -match "_tests?$") {
    throw "Refusing to bootstrap a database named '$Db': the API refuses to serve from a *_test database, and tests own those names. Use tira_dev."
}

# ---------------------------------------------------------------- toolchain
# dev.ps1 documents why this machine needs a real MinGW-w64: rustup's bundled
# dlltool cannot create import libraries here.
$mingwCandidates = @(
    "C:\tools\winlibs\bin",
    "C:\tools\mingw64\mingw64\bin",
    "C:\tools\mingw64\bin"
)
$mingw = $mingwCandidates | Where-Object { Test-Path (Join-Path $_ "dlltool.exe") } | Select-Object -First 1
if (-not $mingw) {
    Write-Warning "No MinGW-w64 found; the Rust link step will fail. See scripts/dev.ps1."
}

# ---------------------------------------------------------------- postgres
Write-Host "== PostgreSQL ==" -ForegroundColor Cyan
& (Join-Path $PSScriptRoot "pg-dev.ps1") -Port $DbPort -Db $Db -PgRoot $PgRoot
if ($LASTEXITCODE -ne 0) { throw "pg-dev.ps1 failed with exit code $LASTEXITCODE" }

# pg-dev.ps1 sanitises PATH to the postgres bin + system dirs (a real DLL
# shadowing bug). Restore what this script still needs.
$env:Path = "$pgBin;$env:USERPROFILE\.cargo\bin;$env:Path"
if ($mingw) { $env:Path = "$mingw;$env:Path" }

$dbUrl = "postgres://postgres@127.0.0.1:$DbPort/$Db"

# ---------------------------------------------------------------- port check
function Test-PortInUse([int]$p) {
    $conn = Get-NetTCPConnection -LocalPort $p -State Listen -ErrorAction SilentlyContinue
    return [bool]$conn
}

if (Test-PortInUse $Port) {
    throw "Port $Port is already listening. Stop that process (for example an older tira-api) or pass -Port."
}
if (Test-PortInUse $SmtpPort) {
    throw "Port $SmtpPort is already listening. Stop that process or pass -SmtpPort."
}

# ---------------------------------------------------------------- build
$exe = Join-Path $repo "target\debug\tira-api.exe"
if (-not $NoBuild) {
    Write-Host "== cargo build -p tira-api ==" -ForegroundColor Cyan
    & cargo build -p tira-api --manifest-path (Join-Path $repo "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path $exe)) { throw "Binary not found at $exe. Run without -NoBuild once." }

# ---------------------------------------------------------------- environment
# These are the only knobs this script owns; everything else keeps the
# defaults documented in .env.example.
$env:TIRA_DATABASE_URL   = $dbUrl
$env:TIRA_BIND           = "127.0.0.1:$Port"
$env:TIRA_BLOB_ROOT      = (Join-Path $repo "data\blobs-dev")
$env:TIRA_DEV_MODE       = "1"
$env:TIRA_DEV_DOMAIN     = $Domain
# The session cookie is `Secure` by default, which a browser correctly refuses
# to send over plain HTTP. Loopback is the only place that opt-out is allowed.
$env:TIRA_COOKIE_INSECURE = "1"
$env:TIRA_SMTP_ENABLE    = "1"
$env:TIRA_SMTP_ADDR      = "127.0.0.1:$SmtpPort"
$env:TIRA_SMTP_HOST      = "127.0.0.1"
$env:TIRA_SMTP_PORT      = "$SmtpPort"
$env:TIRA_SMTP_STARTTLS  = "0"
if (-not $env:RUST_LOG) { $env:RUST_LOG = "info" }

# A dedicated blob root matters: the orphan sweep deletes blobs its own
# database does not reference, so sharing `data/blobs` with `tira_test` would
# eventually delete the demo's message bodies.
New-Item -ItemType Directory -Force -Path $env:TIRA_BLOB_ROOT | Out-Null

# ---------------------------------------------------------------- start API
$log = Join-Path $env:TEMP "tira-dev-api.log"
$errLog = Join-Path $env:TEMP "tira-dev-api.err.log"
Remove-Item $log, $errLog -ErrorAction SilentlyContinue

Write-Host "== tira-api ==" -ForegroundColor Cyan
$api = Start-Process -FilePath $exe -WorkingDirectory $repo -PassThru -NoNewWindow `
    -RedirectStandardOutput $log -RedirectStandardError $errLog

try {
    $deadline = (Get-Date).AddSeconds(90)
    $healthy = $false
    while ((Get-Date) -lt $deadline) {
        if ($api.HasExited) {
            throw "tira-api exited with code $($api.ExitCode). See $errLog"
        }
        try {
            $resp = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/healthz" -TimeoutSec 2
            if ($resp.StatusCode -eq 200) { $healthy = $true; break }
        } catch {
            # Not listening yet, or still applying migrations.
        }
        Start-Sleep -Milliseconds 400
    }
    if (-not $healthy) { throw "tira-api did not become healthy in 90s. See $log and $errLog" }

    # ------------------------------------------------------------ bootstrap
    # Migrations have run by now (PgStore::connect applies them), so the tables
    # exist. ON CONFLICT keeps this idempotent across restarts.
    Write-Host "== dev tenant + domain ==" -ForegroundColor Cyan
    $sqlFile = Join-Path $env:TEMP "tira-dev-bootstrap.sql"
    @"
INSERT INTO tenants (id, name, cndp_ref)
VALUES ('7a1e0000-0000-4000-8000-000000000001', 'Tira local development', NULL)
ON CONFLICT (id) DO NOTHING;

INSERT INTO domains (id, tenant_id, name)
VALUES ('7a1e0000-0000-4000-8000-000000000002', '7a1e0000-0000-4000-8000-000000000001', '$Domain')
ON CONFLICT DO NOTHING;
"@ | Set-Content -Path $sqlFile -Encoding ASCII

    & $psql -h 127.0.0.1 -p $DbPort -U postgres -w -d $Db -v ON_ERROR_STOP=1 -q -f $sqlFile
    if ($LASTEXITCODE -ne 0) { throw "bootstrap SQL failed; see the psql output above" }

    $counts = & $psql -h 127.0.0.1 -p $DbPort -U postgres -w -d $Db -tAc `
        "SELECT 'tenants=' || (SELECT count(*) FROM tenants) || ' mailboxes=' || (SELECT count(*) FROM mailboxes) || ' messages=' || (SELECT count(*) FROM messages)"
    Write-Host "  $($counts.Trim())" -ForegroundColor Gray

    $domainOwner = & $psql -h 127.0.0.1 -p $DbPort -U postgres -w -d $Db -tAc `
        "SELECT tenant_id FROM domains WHERE name = '$Domain'"
    if ("$domainOwner".Trim() -ne "7a1e0000-0000-4000-8000-000000000001") {
        Write-Warning "Domain $Domain is owned by tenant '$("$domainOwner".Trim())', not the dev tenant. Signups will join that tenant."
    }

    Write-Host ""
    Write-Host "Tira Mail is running." -ForegroundColor Green
    Write-Host "  Sign up:  http://127.0.0.1:$Port/signup" -ForegroundColor Green
    Write-Host "  Log in:   http://127.0.0.1:$Port/login" -ForegroundColor Green
    Write-Host "  Database: $dbUrl" -ForegroundColor Gray
    Write-Host "  Blobs:    $($env:TIRA_BLOB_ROOT)" -ForegroundColor Gray
    Write-Host "  Logs:     $log" -ForegroundColor Gray
    Write-Host ""
    Write-Host "Create a mailbox, then use 'send to yourself' to exercise the whole path" -ForegroundColor Gray
    Write-Host "(compose -> relay 127.0.0.1:$SmtpPort -> tira-smtp -> PostgreSQL + blob)." -ForegroundColor Gray
    Write-Host "Press Ctrl+C to stop the API." -ForegroundColor Gray

    Wait-Process -Id $api.Id
}
finally {
    if ($api -and -not $api.HasExited) {
        Write-Host "Stopping tira-api (pid $($api.Id))..." -ForegroundColor Yellow
        Stop-Process -Id $api.Id -Force -ErrorAction SilentlyContinue
    }
}
