#requires -Version 5.1
<#
    Tira Mail - persistent Stalwart admin account (bootstrap).

    Why this exists
    ---------------
    Stalwart's recovery admin (STALWART_RECOVERY_ADMIN) is not a durable
    account: it only exists while the server runs in recovery mode. In normal
    mode `stalwart-cli` therefore fails with 401 until a real account with the
    Admin role exists. This script creates that account once, from the
    committed plan template `deploy/stalwart/admin.ndjson`.

    The role variant matters. On Stalwart 1.0.2 the value is:

        "roles": { "@type": "Admin" }

    `"Administrator"` is rejected with:
        invalidPatch ... Properties: roles

    Preconditions
    -------------
      * Stalwart is running in recovery mode and its management listener is
        reachable (default http://127.0.0.1:8080, plain HTTP).
      * The target domain already exists in the engine (e.g. tira.test).
      * STALWART_RECOVERY_PASSWORD / STALWART_ADMIN_PASSWORD are set, or the
        script prompts. Passwords are never taken on the command line: argv is
        visible in the process list.

    After this script
    -----------------
    Restart Stalwart WITHOUT STALWART_RECOVERY_MODE, then give the control plane:

        STALWART_URL      = http://127.0.0.1:8080
        STALWART_USER     = admin@tira.test
        STALWART_PASSWORD = <the secret you just set>

    Re-running the script rotates the password; the account id is preserved.

    Usage
    -----
        $env:STALWART_RECOVERY_PASSWORD = "..."
        $env:STALWART_ADMIN_PASSWORD    = "..."
        .\scripts\stalwart-admin.ps1

        .\scripts\stalwart-admin.ps1 -AdminUser postmaster@uca.ma -DomainName uca.ma
#>
[CmdletBinding()]
param(
    [string] $AdminUser = "admin@tira.test",
    [string] $DomainName = "tira.test",
    [string] $RecoveryUser = "admin",
    [string] $Url = "http://127.0.0.1:8080",
    [string] $Cli = "C:\tools\stalwart\stalwart-cli.exe",
    [string] $PlanTemplate = (Join-Path $PSScriptRoot "..\deploy\stalwart\admin.ndjson")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Read-Secret {
    param([string] $Name, [string] $Prompt)
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ($value) { return $value }
    $secure = Read-Host -Prompt $Prompt -AsSecureString
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try {
        return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
}

if (-not (Test-Path $Cli)) { throw "stalwart-cli not found: $Cli" }
if (-not (Test-Path $PlanTemplate)) { throw "plan template not found: $PlanTemplate" }

$localPart, $adminDomain = $AdminUser.Split("@", 2)
if (-not $localPart -or -not $adminDomain) { throw "AdminUser must be local@domain" }
if ($adminDomain -ne $DomainName) {
    throw "AdminUser domain ($adminDomain) must match -DomainName ($DomainName)"
}

$recoveryPassword = Read-Secret -Name "STALWART_RECOVERY_PASSWORD" -Prompt "Recovery admin password"
$adminPassword = Read-Secret -Name "STALWART_ADMIN_PASSWORD" -Prompt "New password for $AdminUser"
if ($adminPassword.Length -lt 12) {
    throw "refusing a password shorter than 12 characters"
}

# stalwart-cli reads STALWART_PASSWORD from the environment, which keeps the
# secret out of its argv. These process-wide variables are saved here and
# restored in the finally block below, so the admin secret never lingers in the
# caller's session after the script (whether it succeeds or throws).
$savedStalwartEnv = @{
    STALWART_URL      = $env:STALWART_URL
    STALWART_USER     = $env:STALWART_USER
    STALWART_PASSWORD = $env:STALWART_PASSWORD
}
try {
    $env:STALWART_URL = $Url
    $env:STALWART_USER = $RecoveryUser
    $env:STALWART_PASSWORD = $recoveryPassword

    Write-Host "`n[1] Looking up domain $DomainName in the engine" -ForegroundColor Cyan
    $domainRows = & $Cli query Domain --where "name=$DomainName" --fields id,name --json
    if ($LASTEXITCODE -ne 0) { throw "domain query failed (is the server in recovery mode on $Url?)" }
    $domain = $domainRows | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.name -eq $DomainName } | Select-Object -First 1
    if (-not $domain) { throw "domain $DomainName does not exist in the engine; create it first" }
    Write-Host ("  ok  {0} -> engine id {1}" -f $domain.name, $domain.id) -ForegroundColor Green

    # Build the plan from the committed template so the shape stays reviewable in
    # git while the secret never is. ConvertFrom-Json/ConvertTo-Json handles quoting
    # correctly; string interpolation would not.
    $plan = Get-Content -Raw -Encoding UTF8 $PlanTemplate | ConvertFrom-Json
    $reference = $plan.value.PSObject.Properties | Select-Object -First 1
    if (-not $reference) { throw "plan template has an empty value object" }
    $account = $reference.Value
    $account.name = $localPart
    $account.domainId = $domain.id
    $account.credentials.'0'.secret = $adminPassword
    $account.description = "Tira provisioning service account"

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("tira-admin-{0}.ndjson" -f [guid]::NewGuid())
    try {
        $json = $plan | ConvertTo-Json -Depth 12 -Compress
        [IO.File]::WriteAllText($tmp, $json + "`n", (New-Object Text.UTF8Encoding($false)))

        Write-Host "`n[2] Upserting $AdminUser with role Admin" -ForegroundColor Cyan
        & $Cli apply --file $tmp --json
        if ($LASTEXITCODE -ne 0) { throw "admin plan failed (see the engine's error above)" }
    } finally {
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    }

    Write-Host "`n[3] Verifying the account is readable with the new credential" -ForegroundColor Cyan
    $env:STALWART_USER = $AdminUser
    $env:STALWART_PASSWORD = $adminPassword
    $rows = & $Cli query Account --where "name=$localPart" --where "domainId=$($domain.id)" `
        --fields id,name,emailAddress,roles --json
    if ($LASTEXITCODE -ne 0) {
        throw "the account exists but cannot authenticate yet; restart without STALWART_RECOVERY_MODE and retry"
    }
    $created = $rows | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json } | Select-Object -First 1
    if (-not $created) { throw "verification query returned no account" }
    Write-Host ("  ok  {0} (engine id {1})" -f $created.name, $created.id) -ForegroundColor Green

    Write-Host "`nAdmin bootstrap: PASS" -ForegroundColor Green
    Write-Host "Restart Stalwart without STALWART_RECOVERY_MODE, then set:" -ForegroundColor Yellow
    Write-Host "  STALWART_URL=$Url"
    Write-Host "  STALWART_USER=$AdminUser"
    Write-Host "  STALWART_PASSWORD=<the secret you entered>"
}
finally {
    # Restore the caller's prior environment exactly: a previously-unset variable
    # is removed rather than left as an empty string, so no STALWART_PASSWORD
    # lingers in the session after the script.
    foreach ($name in $savedStalwartEnv.Keys) {
        $previous = $savedStalwartEnv[$name]
        if ([string]::IsNullOrEmpty($previous)) {
            Remove-Item -LiteralPath "Env:\$name" -ErrorAction SilentlyContinue
        } else {
            Set-Item -LiteralPath "Env:\$name" -Value $previous
        }
    }
}
