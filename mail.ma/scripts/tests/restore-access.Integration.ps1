#requires -Version 5.1
<#
Real PostgreSQL 16+ / real pg_dump, pg_restore, tar / real optional HTTP process.
Explicit opt-in: requires a lab parent and creates TWO NEW local clusters there.
Never accepts an existing server URL or data directory. SCRAM, loopback, fresh
random password, protected fixture directory. All databases/data are retained;
only the owned test processes stop, and the temporary initdb password is removed.
The fixture is SYNTHETIC, including two MIME messages with binary attachments.
No Stalwart, production mail, outbound SMTP, or encrypted-archive claim.

Example (run in a child Windows PowerShell process):
 powershell -NoProfile -ExecutionPolicy RemoteSigned -File scripts/tests/restore-access.Integration.ps1 -LabParent C:\scratch -ApiExe C:\build\tira-api.exe
Omitting ApiExe explicitly skips HTTP checks (reported in results).
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $LabParent,
    [string] $PgBin='C:\tools\pgsql\bin',
    [string] $ApiExe
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if (-not (Test-Path -LiteralPath $LabParent -PathType Container)) { throw 'LabParent must be an existing directory' }
$LabParent=[IO.Path]::GetFullPath($LabParent)
if ($LabParent.Length -gt 90) { throw 'Use a shorter lab parent (Windows PowerShell/.NET Framework path limits)' }
for ($p=Get-Item -LiteralPath $LabParent; $null -ne $p; $p=$p.Parent) {
    if ($p.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse points are not allowed in lab parent' }
}
foreach ($name in @('initdb','postgres','pg_ctl','psql','createdb','dropdb','pg_dump','pg_restore')) {
    if (-not (Test-Path -LiteralPath (Join-Path $PgBin "$name.exe") -PathType Leaf)) { throw "Missing tool: $name" }
}
if ($ApiExe -and -not (Test-Path -LiteralPath $ApiExe -PathType Leaf)) { throw 'ApiExe not found' }
. (Join-Path $PSScriptRoot 'restore-access.Lab.ps1')
. (Join-Path $PSScriptRoot 'restore-access.Fixture.ps1')
. (Join-Path $PSScriptRoot '..\restore-access.ps1')
$script:Checks=New-Object 'System.Collections.Generic.List[string]'
$app=$null; $http=$null; $failure=$null; $completed=$false
$drill=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\restore-drill.ps1'))
function Check([bool]$Condition,[string]$Name) {
    if (-not $Condition) { throw "FAIL: $Name" }
    $script:Checks.Add($Name); Write-Host "PASS: $Name"
}
function Expect-DrillFailure([scriptblock]$Action,[string]$Pattern) {
    $errorText=$null
    try { & $Action | Out-Null } catch { $errorText=$_.ToString() }
    Check ($null -ne $errorText -and $errorText -match $Pattern) "drill refuses [$Pattern]"
}
function Denied([string]$Query,[string]$Name) {
    $r=Invoke-LabSql $dst 'tira_restore_owner' ("BEGIN; SET LOCAL ROLE tira_app; SELECT set_config('tira.tenant_id','$($Fixtures[0].Tenant)',true); " + $Query + '; ROLLBACK;') 'b2_owner'
    Check ($r.ExitCode -ne 0 -and $r.Output -match 'permission denied|row-level security') $Name
}
function Request([string]$Path,[string]$Token='') {
    $req=[Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get,($script:BaseUrl+$Path))
    if ($Token) { $req.Headers.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',$Token) }
    try {
        $res=$http.SendAsync($req).GetAwaiter().GetResult()
        try {
            $bytes=$res.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
            return [pscustomobject]@{Status=[int]$res.StatusCode;Bytes=$bytes;Text=[Text.Encoding]::UTF8.GetString($bytes)}
        } finally { $res.Dispose() }
    } finally { $req.Dispose() }
}
try {
    Start-Lab
    $src=New-LabCluster src; $dst=New-LabCluster dst
    $blobRoot=New-MailFixture $src
    $backup=Run-BackupFixture $src $blobRoot
    $backupHashes=@(Get-ChildItem -LiteralPath $backup -File | ForEach-Object { @{Path=$_.FullName;Hash=Get-Sha256 $_.FullName} })
    $base=@{BackupDir=$backup;DbHost='127.0.0.1';Port=$dst.Port;DbUser='b2_admin';PgBin=$PgBin;Keep=$true}
    Expect-DrillFailure { & $drill @base -ScratchDb tira_restore_missing } 'tira_app is missing'
    Check ((Sql $dst postgres "SELECT count(*) FROM pg_roles WHERE rolname='tira_app';") -eq '0') 'default does not create a cluster role'
    Check ((Sql $dst postgres "SELECT count(*) FROM pg_database WHERE datname='tira_restore_missing';") -eq '0') 'role refusal precedes database creation'

    & $drill @base -ScratchDb tira_restore_admin -CreateAppRole
    Check ((Sql $dst postgres "SELECT NOT (rolcanlogin OR rolsuper OR rolbypassrls OR rolcreatedb OR rolcreaterole OR rolreplication) FROM pg_roles WHERE rolname='tira_app';") -eq 't') 'explicit role creation is unprivileged NOLOGIN'
    # Provision a distinct non-superuser owner ONLY inside this freshly owned lab.
    Sql $dst postgres "CREATE ROLE b2_owner LOGIN CREATEDB NOSUPERUSER NOBYPASSRLS NOCREATEROLE PASSWORD '$script:Secret'; GRANT tira_app TO b2_owner; CREATE ROLE b2_outsider NOLOGIN;" | Out-Null
    $workBefore=@(Get-ChildItem -LiteralPath $env:TEMP -Directory | ForEach-Object FullName)
    $owner=@{BackupDir=$backup;DbHost='127.0.0.1';Port=$dst.Port;DbUser='b2_owner';PgBin=$PgBin;Keep=$true}
    & $drill @owner -ScratchDb tira_restore_owner
    $newWork=@(Get-ChildItem -LiteralPath $env:TEMP -Directory | Where-Object { $_.FullName -notin $workBefore })
    Check ($newWork.Count -eq 1) 'restore has one newly owned work directory'
    $restoredBlobs=Join-Path $newWork[0].FullName 'blobs'
    Check ((Sql $dst tira_restore_owner "SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='messages'::regclass;") -eq 'b2_owner') 'restored tables belong to the non-superuser owner'
    Check ((Sql $dst postgres "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname='b2_owner';") -eq 'f') 'runtime owner is not superuser and has no BYPASSRLS'

    # Compare EVERY table, not just the original eight aggregate counters.
    $tables=(Sql $src b2_source "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename;") -split '\r?\n'
    foreach ($table in $tables) {
        if ($table -cnotmatch '\A[a-z_]+\z') { throw 'Unexpected fixture table name' }
        $q="SELECT md5(COALESCE(string_agg(row_to_json(t)::text,E'\n' ORDER BY row_to_json(t)::text),'')) FROM public.$table t;"
        Check ((Sql $src b2_source $q) -ceq (Sql $dst tira_restore_owner $q)) "all row values restored: $table"
    }
    foreach ($f in $Fixtures) { Check ((Get-Sha256 (Join-Path $restoredBlobs $f.Key)) -ceq $f.Hash) "MIME plus binary attachment byte hash: $($f.Message)" }
    $url="postgres://b2_owner@127.0.0.1:$($dst.Port)/tira_restore_owner"
    Restore-TiraAccess -Psql (Join-Path $PgBin 'psql.exe') -DatabaseUrl $url
    Check $true 'access policy can be reapplied idempotently'
    foreach ($role in @('tira_app','b2_outsider')) {
        foreach ($fn in @('tira_resolve_token(bytea)','tira_resolve_recipient(text,text)')) {
            Check ((Sql $dst tira_restore_owner "SELECT has_function_privilege('$role','public.$fn','EXECUTE');") -eq 'f') "definer function denied: $role / $fn"
        }
    }
    foreach ($f in $Fixtures) {
        foreach ($table in @('tenants','domains','mailboxes','messages','outbound_queue','bounces')) {
            $q="BEGIN; SET LOCAL ROLE tira_app; SELECT set_config('tira.tenant_id','$($f.Tenant)',true); SELECT 'VISIBLE='||count(*) FROM public.$table; ROLLBACK;"
            Check ((Sql $dst tira_restore_owner $q b2_owner) -match '(?m)^VISIBLE=1\r?$') "tenant sees only its row: $($f.Tenant) / $table"
        }
    }
    $q="BEGIN; SET LOCAL ROLE tira_app; SELECT set_config('tira.tenant_id','',true); SELECT 'VISIBLE='||count(*) FROM messages; ROLLBACK;"
    Check ((Sql $dst tira_restore_owner $q b2_owner) -match '(?m)^VISIBLE=0\r?$') 'unset tenant sees no message'
    Denied 'SELECT dkim_private_enc FROM domains' 'DKIM private column denied'
    Denied 'SELECT token_hash FROM api_tokens' 'token hashes denied'
    Denied 'SELECT password_hash FROM mailbox_credentials' 'credential hashes denied'
    Denied 'SELECT * FROM message_recipients' 'recipient table denied'
    Denied 'SELECT * FROM audit_log' 'audit table remains write-only'
    Denied "SELECT * FROM tira_resolve_token(decode(repeat('00',32),'hex'))" 'direct token definer invocation denied'
    Denied "SELECT * FROM tira_resolve_recipient('test','tenant1.invalid')" 'direct recipient definer invocation denied'
    Denied "INSERT INTO audit_log(tenant_id,actor,action) VALUES ('$($Fixtures[1].Tenant)','fixture','cross-tenant')" 'cross-tenant audit write denied by RLS'
    Denied "UPDATE outbound_queue SET tenant_id='$($Fixtures[1].Tenant)', mailbox_id='$($Fixtures[1].Mailbox)'" 'queue update cannot move a row into another tenant'
    Check ((Sql $dst tira_restore_owner "SELECT has_database_privilege('b2_outsider',current_database(),'CONNECT');") -eq 'f') 'scratch database denies PUBLIC connection'
    Check ((Sql $dst tira_restore_owner "SELECT has_schema_privilege('b2_outsider','public','USAGE');") -eq 'f') 'scratch schema denies PUBLIC usage'

    # Existing-name refusal is measured against real catalog OID + row content.
    $oid=Sql $dst postgres "SELECT oid FROM pg_database WHERE datname='tira_restore_owner';"
    Expect-DrillFailure { & $drill @owner -ScratchDb tira_restore_owner } 'already exists'
    Check ((Sql $dst postgres "SELECT oid FROM pg_database WHERE datname='tira_restore_owner';") -ceq $oid) 'existing scratch database OID unchanged'
    Check ((Sql $dst tira_restore_owner 'SELECT count(*) FROM messages;') -eq '2') 'existing scratch data retained'
    & $drill -BackupDir $backup -DbHost 127.0.0.1 -Port $dst.Port -DbUser b2_owner -PgBin $PgBin -ScratchDb tira_restore_cleanup
    Check ((Sql $dst postgres "SELECT count(*) FROM pg_database WHERE datname='tira_restore_cleanup';") -eq '0') 'successful no-Keep drill drops its own real scratch database'
    Check ((Sql $dst postgres "SELECT oid FROM pg_database WHERE datname='tira_restore_owner';") -ceq $oid) 'cleanup preserves another retained restore database'
    # The only role altered here was created by this test on its own new server.
    Sql $dst postgres 'ALTER ROLE tira_app LOGIN;' | Out-Null
    Expect-DrillFailure { & $drill @base -ScratchDb tira_restore_unsafe -CreateAppRole } 'unsafe existing tira_app'
    Check ((Sql $dst postgres "SELECT rolcanlogin FROM pg_roles WHERE rolname='tira_app';") -eq 't') 'unsafe role is refused, not silently normalised'
    Sql $dst postgres 'ALTER ROLE tira_app NOLOGIN; GRANT b2_outsider TO tira_app;' | Out-Null
    Expect-DrillFailure { & $drill @base -ScratchDb tira_restore_membership } 'unsafe existing tira_app'
    Sql $dst postgres 'REVOKE b2_outsider FROM tira_app;' | Out-Null

    # Unknown schema and re-pinned invalid dump are private fixture copies.
    foreach ($variant in @('schema','dump')) {
        $bad=Join-Path $script:Lab ('bad-'+$variant)
        New-Item -ItemType Directory -Path $bad | Out-Null
        Get-ChildItem -LiteralPath $backup -File | Copy-Item -Destination $bad
        $mp=Join-Path $bad 'manifest.json'; $manifest=Get-Content -LiteralPath $mp -Raw | ConvertFrom-Json
        if ($variant -eq 'schema') { $manifest.database.schema_versions += '9999_unknown' }
        else {
            Write-Utf8NoBom (Join-Path $bad 'db.dump') 'INTENTIONALLY INVALID TEST DUMP'
            foreach ($entry in $manifest.files) { if ($entry.name -eq 'db.dump') { $entry.bytes=(Get-Item -LiteralPath (Join-Path $bad 'db.dump')).Length; $entry.sha256=Get-Sha256 (Join-Path $bad 'db.dump') } }
        }
        Write-Utf8NoBom $mp ($manifest | ConvertTo-Json -Depth 8)
        Write-Utf8NoBom (Join-Path $bad 'manifest.sha256') ((Get-Sha256 $mp)+"  manifest.json`n")
        $pattern=if ($variant -eq 'schema') { 'Unsupported restore access schema' } else { 'pg_restore failed' }
        Expect-DrillFailure { & $drill -BackupDir $bad -DbHost 127.0.0.1 -Port $dst.Port -DbUser b2_owner -PgBin $PgBin -ScratchDb "tira_restore_bad_$variant" -Keep } $pattern
        $count=Sql $dst postgres "SELECT count(*) FROM pg_database WHERE datname='tira_restore_bad_$variant';"
        Check ($count -eq $(if ($variant -eq 'schema') {'0'} else {'1'})) "failure retains only acquired resources: $variant"
    }

    if ($ApiExe) {
        Add-Type -AssemblyName System.Net.Http
        $port=Get-FreePort; $script:BaseUrl="http://127.0.0.1:$port"
        $psi=New-Object Diagnostics.ProcessStartInfo
        $psi.FileName=[IO.Path]::GetFullPath($ApiExe); $psi.WorkingDirectory=$script:Lab
        $psi.UseShellExecute=$false; $psi.CreateNoWindow=$true
        $psi.RedirectStandardOutput=$true; $psi.RedirectStandardError=$true
        $psi.EnvironmentVariables['TIRA_DATABASE_URL']="postgres://b2_owner:$([Uri]::EscapeDataString($script:Secret))@127.0.0.1:$($dst.Port)/tira_restore_owner"
        $psi.EnvironmentVariables['TIRA_BLOB_ROOT']=$restoredBlobs
        $psi.EnvironmentVariables['TIRA_BIND']="127.0.0.1:$port"
        $psi.EnvironmentVariables['TIRA_SMTP_ENABLE']='0'
        $psi.EnvironmentVariables['TIRA_PROOF_DIR']=Join-Path $script:Lab 'no-production-proofs'
        $psi.EnvironmentVariables['RUST_LOG']='info'
        $app=[Diagnostics.Process]::Start($psi)
        $appErr=$app.StandardError.ReadToEndAsync()
        $lines=New-Object 'System.Collections.Generic.List[string]'
        $ready=$false; $deadline=[Diagnostics.Stopwatch]::StartNew()
        while (-not $ready) {
            $remaining=30000-[int]$deadline.ElapsedMilliseconds
            if ($remaining -le 0) { throw 'Test API readiness deadline exceeded' }
            $line=$app.StandardOutput.ReadLineAsync()
            if (-not $line.Wait($remaining)) { throw 'Test API did not report readiness' }
            if ($null -eq $line.Result) { throw 'Test API exited before readiness' }
            $lines.Add($line.Result)
            $ready=$line.Result -match 'tira-api listening'
        }
        $appOut=$app.StandardOutput.ReadToEndAsync()
        $handler=[Net.Http.HttpClientHandler]::new(); $handler.UseProxy=$false; $handler.AllowAutoRedirect=$false
        $http=[Net.Http.HttpClient]::new($handler); $http.Timeout=[TimeSpan]::FromSeconds(15)
        foreach ($f in $Fixtures) {
            $r=Request "/api/v1/mailboxes/$($f.Mailbox)/messages" $f.Token
            Check ($r.Status -eq 200 -and ($r.Text | ConvertFrom-Json).total -eq 1) "live restored inbox: $($f.Mailbox)"
            $r=Request "/api/v1/messages/$($f.Message)/raw" $f.Token
            Check ($r.Status -eq 200 -and [Convert]::ToBase64String($r.Bytes) -ceq [Convert]::ToBase64String($f.Bytes)) "live raw MIME and attachment exact: $($f.Message)"
        }
        $r=Request "/api/v1/mailboxes/$($Fixtures[1].Mailbox)/messages" $Fixtures[0].Token
        Check ($r.Status -eq 404) 'live API refuses another tenant mailbox'
        $r=Request "/api/v1/messages/$($Fixtures[1].Message)/raw" $Fixtures[0].Token
        Check ($r.Status -eq 404) 'live API refuses another tenant raw message'
        $r=Request "/api/v1/messages/$($Fixtures[0].Message)/raw"
        Check ($r.Status -eq 401) 'live API requires authentication'
        $r=Request "/mail/$($Fixtures[0].Mailbox)" $Fixtures[0].Token
        Check ($r.Status -eq 200 -and $r.Text.Contains('SYNTHETIC RESTORE FIXTURE 1') -and -not $r.Text.Contains('SYNTHETIC RESTORE FIXTURE 2')) 'server-rendered inbox works on restored storage'
        Check ((Sql $dst tira_restore_owner 'SELECT count(*) >= 7 FROM audit_log;') -eq 't') 'real reads and denials leave audit records'
    } else { Write-Warning 'ApiExe omitted: HTTP/application checks are explicitly skipped' }
    foreach ($f in $backupHashes) { Check ((Get-Sha256 $f.Path) -ceq $f.Hash) 'original test backup artifact unchanged' }
    $completed=$true
} catch { $failure=$_.ToString(); throw }
finally {
    $cleanupFailures=New-Object 'System.Collections.Generic.List[System.Exception]'
    try {
        if ($http) { $http.Dispose() }
        if ($app) {
            # Exact owned test process only; no name-based kill. No pending queue
            # and no SMTP relay exist in this lab. PostgreSQL stops gracefully.
            if (-not $app.HasExited) { $app.Kill(); $app.WaitForExit() }
            if (Get-Variable appOut -ErrorAction SilentlyContinue) {
                Write-Utf8NoBom (Join-Path $script:Lab 'api.out') (($lines -join "`n")+"`n"+$appOut.Result)
            }
            if (Get-Variable appErr -ErrorAction SilentlyContinue) { [IO.File]::WriteAllText((Join-Path $script:Lab 'api.err'),$appErr.Result,(New-Object Text.UTF8Encoding($false))) }
            $app.Dispose()
        }
    } catch { $cleanupFailures.Add($_.Exception) }
    try { Stop-Lab } catch { $cleanupFailures.Add($_.Exception) }
    if ($script:Lab -and (Test-Path -LiteralPath $script:Lab)) {
        $result=[ordered]@{completed=($completed -and $cleanupFailures.Count -eq 0);failure=$failure;cleanup_failures=@($cleanupFailures | ForEach-Object Message);checks=@($script:Checks);http_requested=[bool]$ApiExe;lab=$script:Lab;clusters=@($script:Clusters | Select-Object Name,Port,Data)}
        Write-Utf8NoBom (Join-Path $script:Lab 'integration-results.json') ($result | ConvertTo-Json -Depth 5)
    }
    if ($cleanupFailures.Count) {
        throw ('Integration cleanup failed. Original: '+$failure+'; cleanup: '+(($cleanupFailures | ForEach-Object Message) -join '; '))
    }
}
Write-Host "RESTORE_ACCESS_INTEGRATION_PASSED=$($script:Checks.Count); http=$([bool]$ApiExe); synthetic_data=true; native_tools=true"
