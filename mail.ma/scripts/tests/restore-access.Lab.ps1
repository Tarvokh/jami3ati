#requires -Version 5.1
# Dot-source only from restore-access.Integration.ps1 (supplies PgBin/LabParent).
# Every cluster is newly initdb'd here, SCRAM protected, loopback only, retained.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '..\backup-common.ps1')
$script:Lab = $null
$script:Clusters = New-Object 'System.Collections.Generic.List[object]'
$script:SavedEnv = @{}
$script:Secret = $null
function Start-Lab {
    foreach ($e in Get-ChildItem Env:) {
        if ($e.Name -match '^(PG|TIRA_)' -or $e.Name -in @('TEMP','TMP')) { $script:SavedEnv[$e.Name]=$e.Value }
    }
    foreach ($key in @($script:SavedEnv.Keys)) { [Environment]::SetEnvironmentVariable($key,$null) }
    $script:Lab = Join-Path $LabParent ('b2-' + [guid]::NewGuid().ToString('N').Substring(0,12))
    if (Test-Path -LiteralPath $script:Lab) { throw 'Lab directory collision' }
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true,$false)
    $acl.SetOwner($sid)
    $rule = New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow')
    $acl.AddAccessRule($rule)
    [IO.Directory]::CreateDirectory($script:Lab,$acl) | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $script:Lab 'tmp') | Out-Null
    $env:TEMP = Join-Path $script:Lab 'tmp'; $env:TMP=$env:TEMP
    $env:PGCONNECT_TIMEOUT='5'
    $env:PGOPTIONS='-c statement_timeout=30000 -c lock_timeout=5000'
    $bytes = New-Object byte[] 32
    $rng=[Security.Cryptography.RandomNumberGenerator]::Create()
    try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
    $script:Secret = [Convert]::ToBase64String($bytes)
    $env:PGPASSWORD=$script:Secret
    Write-Utf8NoBom (Join-Path $script:Lab 'fixture-label.txt') 'SYNTHETIC TEST DATA ONLY. Two disposable PostgreSQL clusters; no user mail. Retained after shutdown.'
    Write-Host "LAB_ROOT=$script:Lab"
}
function Get-FreePort {
    $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
    $listener.Start()
    try { return $listener.LocalEndpoint.Port } finally { $listener.Stop() }
}
function Invoke-LabSql {
    param($Cluster,[string]$Database,[string]$Sql,[string]$User='b2_admin')
    if (-not $script:Clusters.Contains($Cluster)) { throw 'Not a cluster owned by this test invocation' }
    if ($Database -cnotmatch '\A[a-z][a-z0-9_]{0,62}\z' -or $User -cnotmatch '\A[a-z][a-z0-9_]{0,62}\z') { throw 'Unsafe lab SQL connection identifier' }
    $psi=New-Object Diagnostics.ProcessStartInfo
    $psi.FileName=Join-Path $script:PgBin 'psql.exe'
    $psi.Arguments="-X -w -A -t -v ON_ERROR_STOP=1 -h 127.0.0.1 -p $($Cluster.Port) -U $User -d $Database"
    $psi.UseShellExecute=$false; $psi.CreateNoWindow=$true
    $psi.RedirectStandardInput=$true; $psi.RedirectStandardOutput=$true; $psi.RedirectStandardError=$true
    $p=[Diagnostics.Process]::Start($psi)
    try {
        $stdout=$p.StandardOutput.ReadToEndAsync(); $stderr=$p.StandardError.ReadToEndAsync()
        $p.StandardInput.WriteLine($Sql); $p.StandardInput.Close()
        if (-not $p.WaitForExit(60000)) { $p.Kill(); $p.WaitForExit(); throw 'Owned psql worker timed out' }
        $text=($stdout.Result + $stderr.Result).Trim().Replace($script:Secret,'[REDACTED]')
        return [pscustomobject]@{ExitCode=$p.ExitCode; Output=$text}
    } finally { $p.Dispose() }
}
function Sql {
    param($Cluster,[string]$Database,[string]$Query,[string]$User='b2_admin')
    $r=Invoke-LabSql $Cluster $Database $Query $User
    if ($r.ExitCode -ne 0) { throw "Lab SQL failed: $($r.Output)" }
    return $r.Output
}
function New-LabCluster {
    param([ValidateSet('src','dst')] [string]$Name)
    $root=Join-Path $script:Lab $Name
    New-Item -ItemType Directory -Path $root | Out-Null
    $data=Join-Path $root 'data'; $pw=Join-Path $root 'bootstrap.local'
    Write-Utf8NoBom $pw ($script:Secret + "`n")
    try {
        $r=Invoke-Native (Join-Path $script:PgBin 'initdb.exe') @('-D',$data,'-U','b2_admin','--auth=scram-sha-256','--encoding=UTF8','--locale=C',"--pwfile=$pw")
        if ($r.ExitCode -ne 0) { throw "initdb failed: $($r.Output.Replace($script:Secret,'[REDACTED]'))" }
    } finally { if (Test-Path -LiteralPath $pw) { Remove-Item -LiteralPath $pw -Force } }
    $port=Get-FreePort
    Write-Utf8NoBom (Join-Path $data 'pg_hba.conf') "host all all 127.0.0.1/32 scram-sha-256`nhost all all 0.0.0.0/0 reject`nhost all all ::0/0 reject`n"
    # Append only to the freshly created cluster configuration, never a system service.
    [IO.File]::AppendAllText((Join-Path $data 'postgresql.conf'), "`nlisten_addresses = '127.0.0.1'`nport = $port`nssl = off`nlogging_collector = off`nlog_statement = 'none'`n", (New-Object Text.UTF8Encoding($false)))
    $cluster=[pscustomobject]@{Name=$Name;Root=$root;Data=$data;Port=$port;Started=$false}
    $script:Clusters.Add($cluster)
    # Windows pg_ctl leaves a cmd/postgres descendant alive. A PowerShell native
    # pipeline waits on inherited pipe handles; Start-Process -Wait also waits on
    # the descendant tree. Redirect to files and wait only for pg_ctl itself.
    $log=Join-Path $root 'server.log'
    $ctl=Start-Process -FilePath (Join-Path $script:PgBin 'pg_ctl.exe') -ArgumentList @('start','-D',('"'+$data+'"'),'-l',('"'+$log+'"'),'-w','-t','30') -PassThru -NoNewWindow -RedirectStandardOutput (Join-Path $root 'start.out') -RedirectStandardError (Join-Path $root 'start.err')
    try {
        $null=$ctl.Handle # Retain the Windows exit-code handle before the process exits.
        if (-not $ctl.WaitForExit(45000)) { throw 'Owned pg_ctl start timed out; finally will stop its cluster' }
        $ctl.Refresh()
        if ($ctl.ExitCode -ne 0) { throw "pg_ctl start failed (exit=$($ctl.ExitCode)); inspect $root" }
    } finally { $ctl.Dispose() }
    $cluster.Started=$true
    $probe=Sql $cluster 'postgres' "SELECT json_build_object('version',current_setting('server_version'),'data_directory',current_setting('data_directory'),'listen',current_setting('listen_addresses'),'port',inet_server_port(),'user',current_user);"
    $observed=$probe | ConvertFrom-Json
    if ([IO.Path]::GetFullPath($observed.data_directory) -ne [IO.Path]::GetFullPath($data) -or $observed.listen -ne '127.0.0.1' -or $observed.port -ne $port -or $observed.user -ne 'b2_admin') { throw 'Lab server identity mismatch' }
    Write-Host "CLUSTER_VERIFIED=$Name; PostgreSQL=$($observed.version); loopback_port=$port; own_data_directory=true"
    return $cluster
}
function Stop-Lab {
    $failures=New-Object 'System.Collections.Generic.List[string]'
    try {
        foreach ($c in $script:Clusters) {
            # pg_ctl uses the PID file inside only our newly initdb'd data dir.
            if (Test-Path -LiteralPath (Join-Path $c.Data 'postmaster.pid')) {
                $r=Invoke-Native (Join-Path $script:PgBin 'pg_ctl.exe') @('stop','-D',$c.Data,'-m','fast','-w','-t','30')
                if ($r.ExitCode -ne 0) { $failures.Add("$($c.Name): $($r.Output)") }
            }
            if (Test-Path -LiteralPath (Join-Path $c.Data 'postmaster.pid')) { $failures.Add("$($c.Name): PID file still present") }
        }
        Write-Host "LAB_RETAINED=$script:Lab"
    } finally {
        foreach ($e in @(Get-ChildItem Env:)) { if ($e.Name -match '^(PG|TIRA_)' -or $e.Name -in @('TEMP','TMP')) { [Environment]::SetEnvironmentVariable($e.Name,$null) } }
        foreach ($key in $script:SavedEnv.Keys) { [Environment]::SetEnvironmentVariable($key,$script:SavedEnv[$key]) }
        $script:Secret=$null
    }
    if ($failures.Count) { throw ('Lab shutdown failed: ' + ($failures -join '; ')) }
    Write-Host 'OWNED_CLUSTERS_STOPPED=true; existing_services_untouched=true'
}
