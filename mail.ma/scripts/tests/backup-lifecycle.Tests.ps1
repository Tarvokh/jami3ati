#requires -Version 5.1
<#
    Executable lifecycle regression tests, not a real database/engine restore proof.
    Only this file is launched as a child process. The actual entrypoints run INSIDE
    its worker mocks. SQL, native tools, tar format and the engine stop signal are
    simulations; directory operations, copying, manifests and hashes are real.

    Run when ready to collect a baseline (no Pester dependency):
      powershell -NoProfile -ExecutionPolicy RemoteSigned -File scripts/tests/backup-lifecycle.Tests.ps1

    Every fixture is below one fresh temp root. Worker-local, constant AllScope
    aliases survive backup-common.ps1 dot-sourcing. Native dispatch has NO fallback.
    TEMP/TMP and PGPASSWORD are process-local: concurrent workers do not share them.
    Wait handles provide a bounded native-work barrier; there are no sleeps/polls.
    Expected lifecycle failures are caught in workers; assertions determine exit 1.
#>
[CmdletBinding()]
param(
    [switch] $Worker,
    [string] $SuiteRoot,
    [string] $WorkerId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-Lifecycle {
    param([bool] $Condition, [string] $Message)
    if (-not $Condition) { throw $Message }
}

function Write-FixtureText {
    param([string] $Path, [string] $Text)
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding($false)))
}

function Get-FixtureHash {
    param([string] $Path)
    (Microsoft.PowerShell.Utility\Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Invoke-LifecycleWorker {
    # Validate the worker envelope before reading any supplied path. All paths to
    # production source are derived from THIS file, never from the request JSON.
    Assert-Lifecycle ($WorkerId -cmatch '^[0-9a-f]{32}$') 'Invalid worker id'
    $baseTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/')
    $rootFull = [IO.Path]::GetFullPath($SuiteRoot).TrimEnd('\', '/')
    Assert-Lifecycle ((Split-Path $rootFull -Parent) -ieq $baseTemp) 'Worker root must be directly under temp'
    Assert-Lifecycle ((Split-Path $rootFull -Leaf) -cmatch '^tira-lifecycle-[0-9a-f]{32}$') 'Invalid suite root'
    Assert-Lifecycle (([IO.File]::ReadAllText((Join-Path $rootFull 'fixture.marker'))) -ceq (Split-Path $rootFull -Leaf)) 'Missing fixture marker'
    $workerRoot = Join-Path $rootFull "workers\$WorkerId"
    $requestPath = Join-Path $workerRoot 'request.json'
    $request = [IO.File]::ReadAllText($requestPath) | ConvertFrom-Json
    Assert-Lifecycle ($request.Kind -cin @('backup', 'engine', 'restore')) 'Unknown entrypoint'

    $global:LifecycleState = [ordered]@{
        Kind = $request.Kind; Mode = $request.Mode; Root = $rootFull
        WorkerRoot = $workerRoot; OutDir = [string]$request.OutDir
        # Keep full blob paths below .NET Framework's legacy MAX_PATH. A fresh,
        # exclusive directory still owns the shortened random leaf; never -Force.
        Temp = Join-Path $rootFull ('t-' + $WorkerId.Substring(0, 12)); PgBin = Join-Path $workerRoot 'bin'
        Input = Join-Path $workerRoot 'input'; BackupDir = Join-Path $workerRoot 'archive'
        WorkerId = $WorkerId; Failed = $false; Error = ''; PrimarySeen = $false
        PasswordDuringPrimary = $false; PasswordChecked = $false; PasswordRestored = $false
        ReachedBarrier = $false; BarrierReleased = $false; CollisionSentinel = ''
        CollisionBytes = [Convert]::ToBase64String([byte[]]@(0, 255, 13, 10, 128, 1, 42))
        Acquisitions = New-Object 'System.Collections.Generic.List[object]'
        OwnedTargets = New-Object 'System.Collections.Generic.List[string]'
        OwnedStages = New-Object 'System.Collections.Generic.List[string]'
        WorkDirs = New-Object 'System.Collections.Generic.List[string]'
        Removals = New-Object 'System.Collections.Generic.List[object]'
        Events = New-Object 'System.Collections.Generic.List[object]'
        SafetyErrors = New-Object 'System.Collections.Generic.List[string]'
        Assertions = New-Object 'System.Collections.Generic.List[string]'
        Catalog = @{}; Scratch = ''; Created = $false; OidReads = 0
        BlobKey = ''; BlobSha = ''; BlobBytes = ''; BlobSize = 0; Counts = ''
    }
    $s = $global:LifecycleState

    function global:Deny-Lifecycle {
        param([string] $Message)
        $global:LifecycleState.SafetyErrors.Add($Message)
        throw "HARNESS SAFETY: $Message"
    }

    function global:Assert-FixturePath {
        param([string] $Path)
        if (-not $Path -or -not [IO.Path]::IsPathRooted($Path) -or $Path -match '[*?\[\]]') {
            Deny-Lifecycle "Non-literal/non-absolute fixture path: $Path"
        }
        $full = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
        $root = $global:LifecycleState.Root
        if ($full -ine $root -and -not $full.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
            Deny-Lifecycle "Path outside fixture root: $Path"
        }
        # Refuse junctions/symlinks in fixtures, including their existing ancestors.
        $cursor = $full
        while ($cursor.Length -ge $root.Length) {
            if (Microsoft.PowerShell.Management\Test-Path -LiteralPath $cursor) {
                $item = Microsoft.PowerShell.Management\Get-Item -LiteralPath $cursor -Force
                if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                    Deny-Lifecycle "Reparse point in fixture: $cursor"
                }
            }
            if ($cursor -ieq $root) { break }
            $cursor = Split-Path $cursor -Parent
        }
    }

    function global:Assert-FixturePaths {
        param([string[]] $Paths)
        foreach ($p in $Paths) { Assert-FixturePath $p }
    }

    function global:Write-WorkerText {
        param([string] $Path, [string] $Text)
        Assert-FixturePath $Path
        Write-FixtureText $Path $Text
    }

    function global:Add-LifecycleEvent {
        param([string] $Name, [string] $Db = '', [string] $Oid = '', [string[]] $Arguments = @())
        $global:LifecycleState.Events.Add([pscustomobject]@{
            Name = $Name; Db = $Db; Oid = $Oid; Arguments = @($Arguments)
        })
    }

    # Mock functions live in this CHILD's global scope: module cmdlets such as
    # Get-FileHash internally resolve Resolve-Path through the global alias and
    # cannot see worker-local functions. No state/aliases escape this process.
    # Qualified cmdlets below are the ONLY filesystem delegation points. None of
    # the production filesystem aliases delegates without validating its paths.
    function global:New-LifecycleItem {
        [CmdletBinding()]
        param([string[]] $Path, [string] $ItemType, [switch] $Force)
        Assert-FixturePaths $Path
        if ($ItemType -ine 'Directory' -or $Path.Count -ne 1) { Deny-Lifecycle 'Only single directory creation is supported' }
        $s = $global:LifecycleState
        $full = [IO.Path]::GetFullPath($Path[0]).TrimEnd('\', '/')
        $parent = Split-Path $full -Parent
        $role = ''
        if ($s.Kind -in @('backup', 'engine') -and $parent -ieq $s.OutDir) { $role = 'target' }
        if ($parent -ieq $s.Temp) {
            if ($s.Kind -eq 'engine') { $role = 'stage' }
            if ($s.Kind -eq 'restore') { $role = 'work' }
        }
        if ($role) {
            $s.Acquisitions.Add([pscustomobject]@{ Role = $role; Path = $full; Force = [bool]$Force })
        }
        if (($role -eq 'target' -and $s.Mode -eq 'CollisionTarget') -or
            ($role -eq 'stage' -and $s.Mode -eq 'CollisionStage')) {
            # Acquisition race: the candidate did not exist at Test-Path time.
            # Another owner creates it just before the exclusive New-Item fails.
            $nested = Join-Path $full 'foreign\nested'
            Assert-FixturePath $nested
            Microsoft.PowerShell.Management\New-Item -ItemType Directory -Path $nested -Force -ErrorAction Stop | Out-Null
            $s.CollisionSentinel = Join-Path $nested 'sentinel.bin'
            [IO.File]::WriteAllBytes($s.CollisionSentinel, [Convert]::FromBase64String($s.CollisionBytes))
            Add-LifecycleEvent 'collision' -Arguments @($full)
            throw 'INJECT acquisition collision after foreign sentinel creation'
        }
        $result = Microsoft.PowerShell.Management\New-Item @PSBoundParameters
        if ($role -eq 'target') { $s.OwnedTargets.Add($full) }
        if ($role -eq 'stage') { $s.OwnedStages.Add($full) }
        if ($role -eq 'work') { $s.WorkDirs.Add($full) }
        $result
    }

    function global:Remove-LifecycleItem {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path, [switch] $Recurse, [switch] $Force)
        $paths = @($LiteralPath) + @($Path) | Where-Object { $_ }
        Assert-FixturePaths $paths
        $s = $global:LifecycleState
        foreach ($p in $paths) {
            if ([IO.Path]::GetFullPath($p).TrimEnd('\', '/') -ieq $s.Root) { Deny-Lifecycle 'Entrypoint tried to remove the suite root' }
            $s.Removals.Add([pscustomobject]@{ Path = $p; ErrorAction = [string]$ErrorActionPreference })
            if ($p -in $s.OwnedTargets -and $s.Mode -in @('PasswordCleanupThrow', 'PrimaryBothCleanupThrow')) {
                throw 'INJECT target cleanup exception'
            }
            if ($p -in $s.WorkDirs -and $s.Mode -eq 'WorkCleanupError') {
                Write-Error 'INJECT work cleanup error'
                return
            }
            if ($p -in $s.OwnedStages -and $s.Mode -in @('PrimaryStageThrow', 'PrimaryBothCleanupThrow')) {
                throw 'INJECT stage cleanup exception'
            }
            if ($p -in $s.OwnedStages -and $s.Mode -eq 'StageCleanupError') {
                # Respect the caller's ErrorAction: old SilentlyContinue wrongly
                # reports success; the fixed -ErrorAction Stop must propagate.
                Write-Error 'INJECT stage cleanup error'
                return
            }
        }
        Microsoft.PowerShell.Management\Remove-Item @PSBoundParameters
    }

    function global:Test-LifecyclePath {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path, [string] $PathType)
        Assert-FixturePaths (@($LiteralPath) + @($Path) | Where-Object { $_ })
        Microsoft.PowerShell.Management\Test-Path @PSBoundParameters
    }
    function global:Get-LifecycleItem {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path, [switch] $Force)
        Assert-FixturePaths (@($LiteralPath) + @($Path) | Where-Object { $_ })
        Microsoft.PowerShell.Management\Get-Item @PSBoundParameters
    }
    function global:Get-LifecycleChildren {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path, [switch] $Recurse,
              [switch] $File, [switch] $Directory, [switch] $Force, [string] $Filter)
        Assert-FixturePaths (@($LiteralPath) + @($Path) | Where-Object { $_ })
        Microsoft.PowerShell.Management\Get-ChildItem @PSBoundParameters
    }
    function global:Get-LifecycleContent {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path, [switch] $Raw)
        Assert-FixturePaths (@($LiteralPath) + @($Path) | Where-Object { $_ })
        Microsoft.PowerShell.Management\Get-Content @PSBoundParameters
    }
    function global:Copy-LifecycleItem {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path, [string] $Destination,
              [switch] $Recurse, [switch] $Force)
        Assert-FixturePaths (@($LiteralPath) + @($Path) + @($Destination) | Where-Object { $_ })
        Microsoft.PowerShell.Management\Copy-Item @PSBoundParameters
    }
    function global:Resolve-LifecyclePath {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path)
        Assert-FixturePaths (@($LiteralPath) + @($Path) | Where-Object { $_ })
        Microsoft.PowerShell.Management\Resolve-Path @PSBoundParameters
    }
    function global:Get-LifecycleHash {
        [CmdletBinding()]
        param([string[]] $LiteralPath, [string[]] $Path, [string] $Algorithm)
        Assert-FixturePaths (@($LiteralPath) + @($Path) | Where-Object { $_ })
        Microsoft.PowerShell.Utility\Get-FileHash @PSBoundParameters
    }
    function global:Write-LifecycleUtf8 {
        param([string] $Path, [string] $Text)
        Write-WorkerText $Path $Text
    }
    function global:Write-LifecycleWarning {
        [CmdletBinding()]
        param([string] $Message)
        if ($global:LifecycleState.Mode -eq 'PasswordWarningThrow' -and $global:LifecycleState.PrimarySeen) {
            Add-LifecycleEvent 'warning-failure'
            throw 'INJECT warning exception during failure cleanup'
        }
        Microsoft.PowerShell.Utility\Write-Warning @PSBoundParameters
    }
    function global:Resolve-LifecycleTar { Join-Path $global:LifecycleState.PgBin 'tar.exe' }
    function global:Test-LifecycleStopped {
        param([string] $ProcessName, [string] $ManagementUrl, [string] $DataDir)
        Assert-FixturePath $DataDir
        Add-LifecycleEvent 'simulated-stopped'
        return $true  # No process discovery, socket, LOCK open or engine execution.
    }

    function global:Get-NativeOption {
        param([string[]] $Arguments, [string[]] $Names)
        for ($i = 0; $i -lt $Arguments.Count; $i++) {
            foreach ($name in $Names) {
                if ($Arguments[$i] -ceq $name) {
                    if ($i + 1 -ge $Arguments.Count) { Deny-Lifecycle "Missing value for $name" }
                    return $Arguments[$i + 1]
                }
                if ($Arguments[$i].StartsWith($name + '=', [StringComparison]::Ordinal)) {
                    return $Arguments[$i].Substring($name.Length + 1)
                }
            }
        }
        return ''
    }
    function global:Native-Result {
        param([int] $Code = 0, [string] $Text = '')
        [pscustomobject]@{ ExitCode = $Code; Output = $Text }
    }
    function global:Wait-NativeBarrier {
        if ($null -eq $global:LifecycleReady) { return }
        $global:LifecycleState.ReachedBarrier = $true
        [void]$global:LifecycleReady.Set()
        if (-not $global:LifecycleRelease.WaitOne(30000)) { throw 'HARNESS barrier timed out' }
        $global:LifecycleState.BarrierReleased = $true
    }
    function global:Test-PrimaryFailure {
        $s = $global:LifecycleState
        if ($s.Mode -in @('PrimaryFail', 'PasswordCleanupThrow', 'PasswordWarningThrow', 'PrimaryStageThrow', 'PrimaryBothCleanupThrow')) {
            $s.PrimarySeen = $true
            $s.PasswordDuringPrimary = ([Environment]::GetEnvironmentVariable('PGPASSWORD') -ceq 'fixture-secret!')
            Add-LifecycleEvent 'primary-failure'
            return $true
        }
        return $false
    }

    function global:Invoke-LifecycleNative {
        param([Parameter(Mandatory)][string] $Exe, [Parameter(Mandatory)][string[]] $Arguments)
        $s = $global:LifecycleState
        Assert-FixturePath $Exe
        $tool = [IO.Path]::GetFileName($Exe).ToLowerInvariant()
        $allowed = @('psql.exe', 'pg_dump.exe', 'pg_restore.exe', 'createdb.exe', 'dropdb.exe', 'tar.exe')
        if ($tool -notin $allowed -or [IO.Path]::GetFullPath($Exe) -ine (Join-Path $s.PgBin $tool)) {
            Deny-Lifecycle "Unrecognized native executable: $Exe"
        }
        Add-LifecycleEvent 'native' -Arguments (@($tool) + $Arguments)
        switch ($tool) {
            'pg_dump.exe' {
                if ($s.Kind -ne 'backup') { Deny-Lifecycle 'pg_dump outside backup scenario' }
                $path = Get-NativeOption $Arguments @('--file', '-f')
                Write-WorkerText $path ("SIMULATED PG DUMP: " + $s.WorkerId)
                Wait-NativeBarrier
                if (Test-PrimaryFailure) { return (Native-Result 91 'INJECT dump failure') }
                return (Native-Result)
            }
            'tar.exe' {
                $create = [Array]::IndexOf($Arguments, '-cf')
                $extract = [Array]::IndexOf($Arguments, '-xf')
                $dir = Get-NativeOption $Arguments @('-C')
                Assert-FixturePath $dir
                if ($create -ge 0 -and $extract -lt 0) {
                    $archive = $Arguments[$create + 1]
                    Assert-FixturePath $archive
                    if ($s.Kind -eq 'engine') {
                        Write-WorkerText $archive 'SIMULATED PARTIAL ENGINE TAR'
                        Wait-NativeBarrier
                        if (Test-PrimaryFailure) { return (Native-Result 92 'INJECT tar failure') }
                    }
                    # This JSON envelope is deliberately NOT a real tar archive.
                    $entries = @(Microsoft.PowerShell.Management\Get-ChildItem -LiteralPath $dir -Recurse -File | ForEach-Object {
                        Assert-FixturePath $_.FullName
                        [pscustomobject]@{
                            name = $_.FullName.Substring($dir.TrimEnd('\', '/').Length).TrimStart('\', '/') -replace '\\', '/'
                            bytes = [Convert]::ToBase64String([IO.File]::ReadAllBytes($_.FullName))
                        }
                    })
                    Write-WorkerText $archive (@{ simulation = 'backup-lifecycle/1'; entries = $entries } | ConvertTo-Json -Depth 6)
                    return (Native-Result)
                }
                if ($extract -ge 0 -and $create -lt 0 -and $s.Kind -eq 'restore') {
                    $archive = $Arguments[$extract + 1]
                    Assert-FixturePath $archive
                    $envelope = [IO.File]::ReadAllText($archive) | ConvertFrom-Json
                    if ($envelope.simulation -cne 'backup-lifecycle/1') { Deny-Lifecycle 'Not a synthetic tar envelope' }
                    foreach ($entry in $envelope.entries) {
                        $name = [string]$entry.name
                        if ($name -cnotmatch '^[a-zA-Z0-9._/-]+$' -or $name.StartsWith('/') -or '..' -in $name.Split('/')) {
                            Deny-Lifecycle "Unsafe synthetic archive member: $name"
                        }
                        $dest = Join-Path $dir ($name -replace '/', '\')
                        Assert-FixturePath $dest
                        Microsoft.PowerShell.Management\New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
                        $bytes = [Convert]::FromBase64String([string]$entry.bytes)
                        if ($s.Mode -eq 'BlobVerifyFail') { $bytes[0] = $bytes[0] -bxor 255 }
                        [IO.File]::WriteAllBytes($dest, $bytes)
                    }
                    Add-LifecycleEvent 'extracted'
                    return (Native-Result)
                }
                Deny-Lifecycle 'Unrecognized tar invocation'
            }
            'createdb.exe' {
                if ($s.Kind -ne 'restore') { Deny-Lifecycle 'createdb outside restore scenario' }
                $db = $Arguments[-1]
                $s.Scratch = $db
                Add-LifecycleEvent 'create' $db -Arguments $Arguments
                if ($s.Mode -eq 'CreateRace') {
                    $s.Catalog[$db] = @{ Oid = '702'; Owner = 'racer'; Bytes = 'foreign-race-sentinel' }
                    return (Native-Result 1 'INJECT concurrent createdb collision')
                }
                if ($s.Catalog.ContainsKey($db)) { return (Native-Result 1 'database already exists') }
                $s.Catalog[$db] = @{ Oid = '701'; Owner = 'worker'; Bytes = 'own-synthetic-db' }
                $s.Created = $true
                Add-LifecycleEvent 'created' $db '701'
                return (Native-Result)
            }
            'dropdb.exe' {
                if ($s.Kind -ne 'restore') { Deny-Lifecycle 'dropdb outside restore scenario' }
                $db = $Arguments[-1]
                $oid = ''
                if ($s.Catalog.ContainsKey($db)) { $oid = $s.Catalog[$db].Oid }
                Add-LifecycleEvent 'drop' $db $oid $Arguments
                if ($s.Mode -eq 'DropFail' -and $s.Created) { return (Native-Result 1 'INJECT dropdb failure') }
                # Deliberately model old unsafe pre-drop instead of hiding it behind
                # a mock refusal; assertions must detect destruction of fake owners.
                [void]$s.Catalog.Remove($db)
                return (Native-Result)
            }
            'pg_restore.exe' {
                if ($s.Kind -ne 'restore') { Deny-Lifecycle 'pg_restore outside restore scenario' }
                $db = Get-NativeOption $Arguments @('--dbname', '-d')
                Assert-FixturePath $Arguments[-1]
                Add-LifecycleEvent 'restore' $db -Arguments $Arguments
                if (-not $s.Catalog.ContainsKey($db)) { Deny-Lifecycle 'Restore targeted an absent synthetic database' }
                if ($s.Mode -eq 'RestoreFail') { return (Native-Result 1 'INJECT pg_restore failure') }
                if ($s.Mode -eq 'ChangedOid') {
                    $s.Catalog[$db] = @{ Oid = '703'; Owner = 'replacement'; Bytes = 'foreign-replacement-sentinel' }
                    Add-LifecycleEvent 'replaced' $db '703'
                }
                return (Native-Result)
            }
            'psql.exe' {
                $sql = Get-NativeOption $Arguments @('--command', '-c')
                $db = Get-NativeOption $Arguments @('--dbname', '-d')
                if ($sql -match "(?is)^\s*SELECT\s+oid\s+FROM\s+pg_database\s+WHERE\s+datname\s*=\s*'([^']+)'\s*;?\s*$") {
                    $name = $Matches[1]
                    if ($s.Kind -ne 'restore' -or ($db -cne 'postgres' -and $db -cnotmatch '/postgres$')) {
                        Deny-Lifecycle 'OID lookup must use the postgres maintenance database'
                    }
                    $s.Scratch = $name
                    $s.OidReads++
                    $oid = ''
                    if ($s.Catalog.ContainsKey($name)) { $oid = $s.Catalog[$name].Oid }
                    Add-LifecycleEvent 'oid' $name $oid $Arguments
                    if ($s.Mode -eq 'QueryFail' -or ($s.Mode -eq 'CaptureQueryFail' -and $s.Created)) {
                        return (Native-Result 2 'INJECT catalog query failure')
                    }
                    if ($s.Mode -eq 'MalformedOid') { return (Native-Result 0 'not-an-oid') }
                    if ($s.Mode -eq 'CaptureBlankOid' -and $s.Created) { return (Native-Result 0 '') }
                    return (Native-Result 0 $oid)
                }
                # Match only the known SQL shapes: unknown queries fail closed.
                if ($sql -ceq 'SHOW server_version' -and $s.Kind -eq 'backup') { return (Native-Result 0 '16.simulated') }
                if ($sql.StartsWith('-- tira-restore-')) {
                    # SQL execution remains simulated here; the real PostgreSQL
                    # integration suite verifies the access contract itself.
                    if ($s.Kind -ne 'restore') { Deny-Lifecycle 'Restore access SQL outside restore' }
                    if ($sql.StartsWith('-- tira-restore-role/1') -and $db -ceq 'postgres' -and -not $s.Created) {
                        Add-LifecycleEvent 'role-preflight'
                        if ($s.Mode -eq 'RolePreflightFail') { return (Native-Result 1 'INJECT role preflight failure') }
                        return (Native-Result)
                    }
                    if ($s.Created -and $db -cmatch ('/' + [regex]::Escape($s.Scratch) + '$')) {
                        if ($sql.StartsWith('-- tira-restore-quarantine/1')) {
                            Add-LifecycleEvent 'quarantine'
                            if ($s.Mode -eq 'QuarantineFail') { return (Native-Result 1 'INJECT quarantine failure') }
                            return (Native-Result)
                        }
                        if ($sql.StartsWith('-- tira-restore-access/1')) {
                            Add-LifecycleEvent 'access'
                            if ($s.Mode -eq 'AccessFail') { return (Native-Result 1 'INJECT access failure') }
                            return (Native-Result)
                        }
                    }
                    Deny-Lifecycle 'Unrecognized restore access SQL or target'
                }
                if ($sql -ceq 'SELECT version FROM schema_version ORDER BY version') { return (Native-Result 0 "0001_init`n0002_engine`n0003_tokens`n0004_rls`n0005_audit`n0006_identity`n0007_column_grants`n0008_outbound_queue`n0009_bounces") }
                if ($sql -match '^SELECT json_build_object\(') { return (Native-Result 0 $s.Counts) }
                if ($sql -ceq 'SELECT blob_key, size_bytes FROM messages GROUP BY blob_key, size_bytes ORDER BY blob_key') {
                    return (Native-Result 0 ("{0}|{1}" -f $s.BlobKey, $s.BlobSize))
                }
                if ($sql -ceq 'SELECT blob_key, sha256, size_bytes FROM messages') {
                    return (Native-Result 0 ("{0}|{1}|{2}" -f $s.BlobKey, $s.BlobSha, $s.BlobSize))
                }
                if (($sql -match '(?s)^SELECT count\(\*\) FROM \(.*s\.used_bytes <> s\.actual\s*$') -or
                    ($sql -match '(?s)^SELECT count\(\*\) FROM messages msg.*msg\.tenant_id <> mb\.tenant_id\s*$') -or
                    ($sql -ceq 'SELECT count(*) FROM api_tokens WHERE octet_length(token_hash) <> 32')) {
                    return (Native-Result 0 '0')
                }
                Deny-Lifecycle "Unrecognized simulated SQL: $sql"
            }
        }
        Deny-Lifecycle "No dispatcher branch for $tool"
    }

    Assert-FixturePath $workerRoot
    Assert-FixturePath $s.OutDir
    foreach ($dir in @($s.Temp, $s.PgBin, $s.Input, $s.BackupDir)) {
        Assert-FixturePath $dir
        Microsoft.PowerShell.Management\New-Item -ItemType Directory -Path $dir | Out-Null
    }
    # Inert files satisfy tool existence checks; these are not executable images.
    foreach ($tool in @('psql.exe', 'pg_dump.exe', 'pg_restore.exe', 'createdb.exe', 'dropdb.exe', 'tar.exe')) {
        Write-WorkerText (Join-Path $s.PgBin $tool) 'INERT TEST FIXTURE - NEVER EXECUTE'
    }
    $blob = [Text.Encoding]::UTF8.GetBytes('Synthetic RFC822 fixture; no real mail.' + $WorkerId)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $s.BlobSha = ([BitConverter]::ToString($sha.ComputeHash($blob))).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
    $s.BlobKey = '{0}/{1}/{2}' -f $s.BlobSha.Substring(0, 2), $s.BlobSha.Substring(2, 2), $s.BlobSha
    $s.BlobSize = $blob.Length
    $s.BlobBytes = [Convert]::ToBase64String($blob)
    $blobPath = Join-Path $s.Input ($s.BlobKey -replace '/', '\')
    Assert-FixturePath $blobPath
    Microsoft.PowerShell.Management\New-Item -ItemType Directory -Path (Split-Path $blobPath -Parent) -Force | Out-Null
    [IO.File]::WriteAllBytes($blobPath, $blob)
    $counts = [ordered]@{ tenants = '1'; domains = '1'; mailboxes = '1'; messages = '1'; recipients = '1'; tokens = '1'; message_bytes = [string]$blob.Length; used_bytes = [string]$blob.Length }
    $s.Counts = $counts | ConvertTo-Json -Compress

    if ($s.Kind -eq 'engine') {
        $data = Join-Path $s.Input 'data'
        Microsoft.PowerShell.Management\New-Item -ItemType Directory -Path $data | Out-Null
        Write-WorkerText (Join-Path $s.Input 'config.json') '{"@type":"RocksDb","path":"data"}'
        Write-WorkerText (Join-Path $data 'CURRENT') "MANIFEST-000001`n"
        foreach ($name in @('IDENTITY', 'MANIFEST-000001', 'OPTIONS-000001', '000001.sst')) {
            Write-WorkerText (Join-Path $data $name) ("SYNTHETIC $name " + $WorkerId)
        }
    }
    if ($s.Kind -eq 'restore') {
        $s.Catalog['tira_restore_foreign'] = @{ Oid = '900'; Owner = 'unrelated'; Bytes = 'unrelated-sentinel' }
        if ($request.ScratchDb) {
            $s.Scratch = [string]$request.ScratchDb
            if ($s.Mode -eq 'Preexisting') {
                $s.Catalog[$s.Scratch] = @{ Oid = '700'; Owner = 'preexisting'; Bytes = 'preexisting-sentinel' }
            }
        }
        Write-WorkerText (Join-Path $s.BackupDir 'db.dump') 'SIMULATED PG RESTORE INPUT'
        $envelope = @{ simulation = 'backup-lifecycle/1'; entries = @(@{ name = $s.BlobKey; bytes = $s.BlobBytes }) }
        Write-WorkerText (Join-Path $s.BackupDir 'blobs.tar') ($envelope | ConvertTo-Json -Depth 6)
        $files = @(foreach ($name in @('db.dump', 'blobs.tar')) {
            $path = Join-Path $s.BackupDir $name
            [ordered]@{ name = $name; bytes = (Microsoft.PowerShell.Management\Get-Item -LiteralPath $path).Length; sha256 = Get-FixtureHash $path }
        })
        $manifest = [ordered]@{
            format = 'tira-backup/1'
            database = @{ consistency = 'quiescent'; counts_before = $counts; counts_after = $counts; schema_versions = @('0001_init','0002_engine','0003_tokens','0004_rls','0005_audit','0006_identity','0007_column_grants','0008_outbound_queue','0009_bounces') }
            files = $files
        }
        $manifestPath = Join-Path $s.BackupDir 'manifest.json'
        Write-WorkerText $manifestPath ($manifest | ConvertTo-Json -Depth 8)
        Write-WorkerText (Join-Path $s.BackupDir 'manifest.sha256') ((Get-FixtureHash $manifestPath) + "  manifest.json`n")
    }

    $global:LifecycleReady = $null
    $global:LifecycleRelease = $null
    if ($request.ReadyEvent) {
        $global:LifecycleReady = [Threading.EventWaitHandle]::OpenExisting([string]$request.ReadyEvent)
        $global:LifecycleRelease = [Threading.EventWaitHandle]::OpenExisting([string]$request.ReleaseEvent)
    }
    $oldTemp = $env:TEMP
    $oldTmp = $env:TMP
    $oldPassword = [Environment]::GetEnvironmentVariable('PGPASSWORD')
    try {
        $env:TEMP = $s.Temp
        $env:TMP = $s.Temp
        $previous = $null
        if ($request.Password -eq 'set') { $previous = 'synthetic-before-worker' }
        [Environment]::SetEnvironmentVariable('PGPASSWORD', $previous)
        $aliases = @{
            'Invoke-Native' = 'Invoke-LifecycleNative'; 'Resolve-Tar' = 'Resolve-LifecycleTar'
            'Test-EngineStopped' = 'Test-LifecycleStopped'; 'New-Item' = 'New-LifecycleItem'
            'Remove-Item' = 'Remove-LifecycleItem'; 'Test-Path' = 'Test-LifecyclePath'
            'Get-Item' = 'Get-LifecycleItem'; 'Get-ChildItem' = 'Get-LifecycleChildren'
            'Get-Content' = 'Get-LifecycleContent'; 'Copy-Item' = 'Copy-LifecycleItem'
            'Resolve-Path' = 'Resolve-LifecyclePath'; 'Get-FileHash' = 'Get-LifecycleHash'
            'Write-Utf8NoBom' = 'Write-LifecycleUtf8'; 'Write-Warning' = 'Write-LifecycleWarning'
        }
        foreach ($name in $aliases.Keys) {
            Set-Alias -Name $name -Value $aliases[$name] -Scope Global -Option Constant,AllScope
        }
        $sourceDir = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
        # Explicitly prove dot-sourcing does not replace the dispatch boundary.
        . (Join-Path $sourceDir 'backup-common.ps1')
        foreach ($name in $aliases.Keys) {
            $resolved = Get-Command $name
            Assert-Lifecycle ($resolved.CommandType -eq 'Alias' -and $resolved.Definition -ceq $aliases[$name]) "Alias lost after common dot-source: $name"
        }
        # No interception of New-BackupDirectory: both target and stage allocation
        # execute the implementation in backup-common when the fix is installed.
        $invoke = @{}
        if ($s.Kind -eq 'backup') {
            $entrypoint = Join-Path $sourceDir 'backup.ps1'
            $invoke = @{ OutDir = $s.OutDir; BlobRoot = $s.Input; PgBin = $s.PgBin; DatabaseUrl = 'postgres://fixture:fixture-secret%21@db.invalid:1/fixture' }
        } elseif ($s.Kind -eq 'engine') {
            $entrypoint = Join-Path $sourceDir 'engine-backup.ps1'
            $invoke = @{ OutDir = $s.OutDir; EngineRunDir = $s.Input; DataDir = (Join-Path $s.Input 'data'); ManagementUrl = 'http://engine.invalid:1'; ProcessName = 'synthetic-never-probed' }
        } else {
            $entrypoint = Join-Path $sourceDir 'restore-drill.ps1'
            $invoke = @{ BackupDir = $s.BackupDir; PgBin = $s.PgBin; DbHost = 'db.invalid'; Port = 1; DbUser = 'fixture' }
            if ($request.ScratchDb) { $invoke.ScratchDb = [string]$request.ScratchDb }
            if ($request.Keep) { $invoke.Keep = $true }
        }
        try { & $entrypoint @invoke | Out-Null }
        catch { $s.Failed = $true; $s.Error = $_.ToString() }

        # This assertion occurs IN THE SAME PROCESS, before harness restoration.
        # A parent checking its own environment could never detect this regression.
        if ($s.Kind -eq 'backup') {
            $s.PasswordChecked = $true
            $actual = [Environment]::GetEnvironmentVariable('PGPASSWORD')
            $s.PasswordRestored = if ($null -eq $previous) { $null -eq $actual } else { $actual -ceq $previous }
            try { Assert-Lifecycle $s.PasswordRestored 'PGPASSWORD was not restored inside the worker' }
            catch { $s.Assertions.Add($_.ToString()) }
        }
    } finally {
        [Environment]::SetEnvironmentVariable('PGPASSWORD', $oldPassword)
        $env:TEMP = $oldTemp
        $env:TMP = $oldTmp
        if ($null -ne $global:LifecycleReady) {
            # Also release the parent's readiness wait when a worker failed before
            # reaching native work. ReachedBarrier distinguishes this from overlap.
            [void]$global:LifecycleReady.Set()
            $global:LifecycleReady.Dispose()
            $global:LifecycleRelease.Dispose()
        }
    }
    Write-WorkerText (Join-Path $workerRoot 'result.json') ($s | ConvertTo-Json -Depth 14)
}

if ($Worker) {
    try { Invoke-LifecycleWorker; exit 0 }
    catch { [Console]::Error.WriteLine("HARNESS WORKER FAILED: $_"); exit 2 }
}
Assert-Lifecycle (-not $SuiteRoot -and -not $WorkerId) 'Worker arguments require -Worker'

$script:passes = 0
$script:failures = 0
function Check {
    param([string] $Name, [scriptblock] $Body)
    try {
        & $Body
        $script:passes++
        Write-Host "  ok  $Name" -ForegroundColor Green
    } catch {
        $script:failures++
        Write-Host "  FAIL $Name : $_" -ForegroundColor Red
    }
}

$root = Join-Path ([IO.Path]::GetTempPath()) ('tira-lifecycle-' + [guid]::NewGuid().ToString('N'))
$root = [IO.Path]::GetFullPath($root)
New-Item -ItemType Directory -Path $root | Out-Null
$workersDir = Join-Path $root 'workers'
New-Item -ItemType Directory -Path $workersDir | Out-Null
Write-FixtureText (Join-Path $root 'fixture.marker') (Split-Path $root -Leaf)
$script:activeWorkers = New-Object 'System.Collections.Generic.List[object]'

function New-Case {
    $path = Join-Path $root ('case-' + [guid]::NewGuid().ToString('N'))
    $out = Join-Path $path 'out'
    $sibling = Join-Path $out 'sibling\nested\sentinel.bin'
    New-Item -ItemType Directory -Path (Split-Path $sibling -Parent) -Force | Out-Null
    [IO.File]::WriteAllBytes($sibling, [byte[]]@(42, 0, 255, 10, 128))
    [pscustomobject]@{ Path = $path; OutDir = $out; Sibling = $sibling; SiblingBytes = [Convert]::ToBase64String([IO.File]::ReadAllBytes($sibling)) }
}

function Start-Worker {
    param([object] $Case, [string] $Kind, [string] $Mode = 'Success',
          [string] $Password = 'absent', [string] $ScratchDb = '', [switch] $Keep,
          [string] $ReadyEvent = '', [string] $ReleaseEvent = '')
    $id = [guid]::NewGuid().ToString('N')
    $dir = Join-Path $workersDir $id
    New-Item -ItemType Directory -Path $dir | Out-Null
    $request = @{ Kind = $Kind; Mode = $Mode; OutDir = $Case.OutDir; Password = $Password;
        ScratchDb = $ScratchDb; Keep = [bool]$Keep; ReadyEvent = $ReadyEvent; ReleaseEvent = $ReleaseEvent }
    Write-FixtureText (Join-Path $dir 'request.json') ($request | ConvertTo-Json)
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = Join-Path $PSHOME 'powershell.exe'
    Assert-Lifecycle (Test-Path -LiteralPath $start.FileName -PathType Leaf) 'This suite requires Windows PowerShell 5.1'
    $start.Arguments = '-NoProfile -ExecutionPolicy RemoteSigned -File "{0}" -Worker -SuiteRoot "{1}" -WorkerId {2}' -f $PSCommandPath, $root, $id
    $start.WorkingDirectory = $dir
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    [void]$process.Start()
    $handle = [pscustomobject]@{
        Process = $process; Directory = $dir
        Stdout = $process.StandardOutput.ReadToEndAsync(); Stderr = $process.StandardError.ReadToEndAsync()
        Finished = $false
    }
    $script:activeWorkers.Add($handle)
    return $handle
}

function Complete-Worker {
    param([object] $Handle)
    $p = $Handle.Process
    if (-not $p.WaitForExit(45000)) {
        $p.Kill() # Only the exact test child that THIS harness started.
        [void]$p.WaitForExit(5000)
        throw 'HARNESS worker exceeded its 45-second deadline'
    }
    $stdout = $Handle.Stdout.GetAwaiter().GetResult()
    $stderr = $Handle.Stderr.GetAwaiter().GetResult()
    $code = $p.ExitCode
    $Handle.Finished = $true
    $p.Dispose()
    Assert-Lifecycle ($code -eq 0) "HARNESS child exit $code : $stderr $stdout"
    $resultPath = Join-Path $Handle.Directory 'result.json'
    Assert-Lifecycle (Test-Path -LiteralPath $resultPath) "HARNESS missing result: $stdout $stderr"
    $result = [IO.File]::ReadAllText($resultPath) | ConvertFrom-Json
    $result | Add-Member -NotePropertyName ConsoleOut -NotePropertyValue $stdout
    Assert-Lifecycle (@($result.SafetyErrors).Count -eq 0) ("HARNESS isolation violation: " + ($result.SafetyErrors -join '; '))
    return $result
}

function Run-Worker {
    param([object] $Case, [string] $Kind, [string] $Mode = 'Success',
          [string] $Password = 'absent', [string] $ScratchDb = '', [switch] $Keep)
    Complete-Worker (Start-Worker @PSBoundParameters)
}
function Events {
    param([object] $Result, [string] $Name)
    @($Result.Events | Where-Object { $_.Name -ceq $Name })
}
function Assert-Sibling {
    param([object] $Case)
    Assert-Lifecycle (Test-Path -LiteralPath $Case.Sibling -PathType Leaf) 'Sibling sentinel was removed'
    Assert-Lifecycle (([Convert]::ToBase64String([IO.File]::ReadAllBytes($Case.Sibling))) -ceq $Case.SiblingBytes) 'Sibling sentinel bytes changed'
}
function Assert-Failure {
    param([object] $Result)
    Assert-Lifecycle $Result.Failed ("Entrypoint returned success in " + $Result.Mode)
    Assert-Lifecycle ([bool]$Result.Error) 'Failure lacked an error'
    Assert-Lifecycle ($Result.ConsoleOut -notmatch '(?m)^(Backup written|Engine backup written|RESTORE DRILL PASSED)\s*$') 'Failure printed a success banner'
}
function Assert-CleanWorker {
    param([object] $Result)
    Assert-Lifecycle (@($Result.Assertions).Count -eq 0) ($Result.Assertions -join '; ')
}
function Assert-OwnOutputRemoved {
    param([object] $Result)
    Assert-Lifecycle (@($Result.OwnedTargets).Count -eq 1) 'Did not reach successful output allocation'
    Assert-Lifecycle (-not (Test-Path -LiteralPath $Result.OwnedTargets[0])) 'Incomplete owned output was left behind'
}
function Assert-CompleteBackup {
    param([object] $Result)
    Assert-Lifecycle (@($Result.OwnedTargets).Count -eq 1) 'Expected exactly one allocated backup'
    $target = $Result.OwnedTargets[0]
    $manifestPath = Join-Path $target 'manifest.json'
    $pinPath = Join-Path $target 'manifest.sha256'
    Assert-Lifecycle (Test-Path -LiteralPath $pinPath -PathType Leaf) 'Complete manifest pin missing'
    $pin = ([IO.File]::ReadAllText($pinPath).Trim() -split '\s+')[0]
    Assert-Lifecycle ($pin -ceq (Get-FixtureHash $manifestPath)) 'Manifest no longer matches its pin'
    $manifest = [IO.File]::ReadAllText($manifestPath) | ConvertFrom-Json
    if ($Result.Kind -eq 'engine') {
        $entries = @($manifest.archive)
        Assert-Lifecycle ($manifest.format -ceq 'tira-engine-backup/1') 'Wrong engine manifest format'
        Assert-Lifecycle ($manifest.inventory.count -ge 6) 'Synthetic engine inventory was not executed'
    } else {
        $entries = @($manifest.files)
        Assert-Lifecycle ($manifest.format -ceq 'tira-backup/1') 'Wrong backup manifest format'
        Assert-Lifecycle ($manifest.blobs.checked -eq 1) 'Real blob preflight did not check the synthetic blob'
    }
    foreach ($entry in $entries) {
        Assert-Lifecycle ([string]$entry.name -cmatch '^[a-zA-Z0-9._-]+$') 'Unsafe output manifest entry'
        $path = Join-Path $target $entry.name
        Assert-Lifecycle ((Get-Item -LiteralPath $path).Length -eq [long]$entry.bytes) 'Output artifact length changed'
        Assert-Lifecycle ((Get-FixtureHash $path) -ceq $entry.sha256) 'Output artifact hash changed'
    }
}
function Get-CatalogEntry {
    param([object] $Result, [string] $Name)
    $property = $Result.Catalog.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}
function Assert-ForeignDatabase {
    param([object] $Result)
    $foreign = Get-CatalogEntry $Result 'tira_restore_foreign'
    Assert-Lifecycle ($null -ne $foreign -and $foreign.Oid -ceq '900' -and $foreign.Bytes -ceq 'unrelated-sentinel') 'Unrelated synthetic database changed'
}
function Assert-NoDrop {
    param([object] $Result)
    Assert-Lifecycle (@(Events $Result 'drop').Count -eq 0) 'A drop was attempted without ownership/permission'
}
function Assert-KeptWork {
    param([object] $Result, [switch] $Extracted)
    Assert-Lifecycle (@($Result.WorkDirs).Count -eq 1) 'Expected a materialized work directory'
    $work = $Result.WorkDirs[0]
    Assert-Lifecycle (Test-Path -LiteralPath (Join-Path $work 'db.dump')) 'Failure/Keep discarded materialized work'
    if ($Extracted) {
        $blob = Join-Path (Join-Path $work 'blobs') ($Result.BlobKey -replace '/', '\')
        Assert-Lifecycle (Test-Path -LiteralPath $blob -PathType Leaf) 'Failure/Keep discarded extracted blob'
    }
}
function Assert-SuccessfulDrop {
    param([object] $Result)
    Assert-Lifecycle (-not $Result.Failed) ("Restore failed: " + $Result.Error)
    $drops = @(Events $Result 'drop')
    Assert-Lifecycle ($drops.Count -eq 1) 'Success must drop once, never pre-drop'
    Assert-Lifecycle ($drops[0].Db -ceq $Result.Scratch -and $drops[0].Oid -ceq '701') 'Drop did not target the database this worker created'
    Assert-Lifecycle ('--force' -notin $drops[0].Arguments -and '--if-exists' -notin $drops[0].Arguments) 'Drop used force/if-exists'
    Assert-Lifecycle ($null -eq (Get-CatalogEntry $Result $Result.Scratch)) 'Owned database remains after reported successful drop'
    $sequence = @($Result.Events | Where-Object { $_.Name -in @('oid', 'create', 'created', 'restore', 'drop') })
    Assert-Lifecycle (($sequence.Name -join ',') -ceq 'oid,create,created,oid,restore,oid,drop') 'Expected absent check, create, OID capture, restore, OID recheck, drop in order'
    Assert-Lifecycle ($sequence[0].Oid -ceq '' -and $sequence[3].Oid -ceq '701' -and $sequence[5].Oid -ceq '701') 'OID ownership sequence incorrect'
    Assert-Lifecycle (@(Events $Result 'extracted').Count -eq 1) 'Restore did not reach blob extraction/verification'
    Assert-Lifecycle (@($Result.WorkDirs).Count -eq 1 -and -not (Test-Path -LiteralPath $Result.WorkDirs[0])) 'Successful drill did not remove its own work'
    Assert-ForeignDatabase $Result
}

try {
    Write-Host 'Backup lifecycle simulation: actual PowerShell control flow; NO real native tools, database or engine.'
    foreach ($kind in @('backup', 'engine')) {
        Check "$kind allocation collision preserves the foreign nested sentinel" {
            $case = New-Case
            $r = Run-Worker $case $kind 'CollisionTarget'
            Assert-Failure $r
            Assert-Lifecycle (@(Events $r 'collision').Count -eq 1) 'Acquisition injection was not reached'
            Assert-Lifecycle (Test-Path -LiteralPath $r.CollisionSentinel -PathType Leaf) 'Foreign collision sentinel was deleted'
            Assert-Lifecycle (([Convert]::ToBase64String([IO.File]::ReadAllBytes($r.CollisionSentinel))) -ceq $r.CollisionBytes) 'Foreign collision bytes changed'
            Assert-Lifecycle (@($r.OwnedTargets).Count -eq 0) 'Failed acquisition was treated as ownership'
            Assert-Sibling $case
            Assert-CleanWorker $r
        }
        Check "$kind primary failure cleans only its allocated output" {
            $case = New-Case
            $r = Run-Worker $case $kind 'PrimaryFail'
            Assert-Failure $r
            Assert-Lifecycle $r.PrimarySeen 'Primary failure injection was not reached'
            Assert-OwnOutputRemoved $r
            foreach ($stage in $r.OwnedStages) { Assert-Lifecycle (-not (Test-Path -LiteralPath $stage)) 'Owned engine stage leaked' }
            Assert-Sibling $case
            Assert-CleanWorker $r
        }
        Check "$kind success writes a pinned complete synthetic backup" {
            $case = New-Case
            $r = Run-Worker $case $kind
            Assert-Lifecycle (-not $r.Failed) $r.Error
            Assert-CompleteBackup $r
            foreach ($stage in $r.OwnedStages) { Assert-Lifecycle (-not (Test-Path -LiteralPath $stage)) 'Successful engine stage leaked' }
            Assert-Sibling $case
            Assert-CleanWorker $r
        }
    }

    Check 'engine stage acquisition collision preserves its foreign sentinel and cleans owned target' {
        $case = New-Case
        $r = Run-Worker $case 'engine' 'CollisionStage'
        Assert-Failure $r
        Assert-Lifecycle (@(Events $r 'collision').Count -eq 1) 'Stage acquisition injection not reached'
        Assert-Lifecycle (Test-Path -LiteralPath $r.CollisionSentinel) 'Foreign stage sentinel was deleted'
        Assert-Lifecycle (([Convert]::ToBase64String([IO.File]::ReadAllBytes($r.CollisionSentinel))) -ceq $r.CollisionBytes) 'Foreign stage bytes changed'
        Assert-OwnOutputRemoved $r
        Assert-Sibling $case
    }
    Check 'engine primary plus stage cleanup exception still attempts target cleanup' {
        $case = New-Case
        $r = Run-Worker $case 'engine' 'PrimaryStageThrow'
        Assert-Failure $r
        Assert-Lifecycle $r.PrimarySeen 'Primary failure not injected'
        Assert-Lifecycle ($r.Error -match 'INJECT tar failure' -and $r.Error -match 'INJECT stage cleanup exception') 'Original and cleanup errors must both be preserved'
        Assert-Lifecycle (@($r.OwnedStages).Count -eq 1) 'No owned stage to inject cleanup failure'
        Assert-Lifecycle ($r.OwnedStages[0] -in @($r.Removals.Path)) 'Stage cleanup was not attempted'
        Assert-Lifecycle ($r.OwnedTargets[0] -in @($r.Removals.Path)) 'Stage cleanup exception skipped target cleanup'
        Assert-OwnOutputRemoved $r
        Assert-Sibling $case
    }
    Check 'engine preserves primary, stage and target errors when both cleanups fail' {
        $case = New-Case
        $r = Run-Worker $case 'engine' 'PrimaryBothCleanupThrow'
        Assert-Failure $r
        Assert-Lifecycle $r.PrimarySeen 'Primary failure not injected'
        foreach ($text in @('INJECT tar failure', 'INJECT stage cleanup exception', 'INJECT target cleanup exception')) {
            Assert-Lifecycle ($r.Error.Contains($text)) "Error was lost: $text"
        }
        Assert-Lifecycle ($r.OwnedStages[0] -in @($r.Removals.Path) -and $r.OwnedTargets[0] -in @($r.Removals.Path)) 'Both cleanups must be attempted'
        Assert-Sibling $case
    }
    Check 'engine completed backup survives stage cleanup error but operation fails' {
        $case = New-Case
        $r = Run-Worker $case 'engine' 'StageCleanupError'
        Assert-Lifecycle (@($r.OwnedStages).Count -eq 1 -and $r.OwnedStages[0] -in @($r.Removals.Path)) 'Stage cleanup error not exercised'
        Assert-CompleteBackup $r
        Assert-Lifecycle ($r.OwnedTargets[0] -notin @($r.Removals.Path)) 'Cleanup deleted a complete engine backup'
        Assert-Failure $r
        Assert-Lifecycle ($r.Error -match 'INJECT stage cleanup error') 'Stage cleanup error was not propagated'
        Assert-Sibling $case
    }

    foreach ($password in @('absent', 'set')) {
        foreach ($mode in @('PasswordCleanupThrow', 'PasswordWarningThrow')) {
            Check "backup restores $password PGPASSWORD after primary + $mode" {
                $case = New-Case
                $r = Run-Worker $case 'backup' $mode -Password $password
                Assert-Failure $r
                Assert-Lifecycle ($r.PrimarySeen -and $r.PasswordDuringPrimary) 'Did not observe the synthetic URL password at the injected primary failure'
                Assert-Lifecycle ($r.PasswordChecked -and $r.PasswordRestored) 'Same-worker PGPASSWORD restoration assertion failed'
                Assert-Lifecycle ($r.Error -match 'INJECT dump failure') 'Cleanup replaced the original dump error'
                Assert-Lifecycle ($r.OwnedTargets[0] -in @($r.Removals.Path)) 'Failure cleanup was not attempted'
                if ($mode -eq 'PasswordWarningThrow') {
                    Assert-Lifecycle (@(Events $r 'warning-failure').Count -gt 0) 'Warning exception was not injected'
                } else {
                    Assert-Lifecycle ($r.Error -match 'INJECT target cleanup exception') 'Cleanup exception was not propagated'
                }
                Assert-Lifecycle ($r.PasswordChecked -and $r.PasswordRestored) 'Same-worker PGPASSWORD restoration assertion failed'
                Assert-CleanWorker $r
                Assert-Sibling $case
            }
        }
    }

    foreach ($kind in @('backup', 'engine')) {
        foreach ($failSecond in @($false, $true)) {
            Check "$kind concurrent overlap, second failure=$failSecond, never removes another output" {
                $case = New-Case
                $tag = [guid]::NewGuid().ToString('N')
                $readyNames = @("Local\TiraLifecycle-$tag-a", "Local\TiraLifecycle-$tag-b")
                $releaseName = "Local\TiraLifecycle-$tag-go"
                $readyA = New-Object Threading.EventWaitHandle($false, [Threading.EventResetMode]::ManualReset, $readyNames[0])
                $readyB = New-Object Threading.EventWaitHandle($false, [Threading.EventResetMode]::ManualReset, $readyNames[1])
                $release = New-Object Threading.EventWaitHandle($false, [Threading.EventResetMode]::ManualReset, $releaseName)
                try {
                    $a = Start-Worker $case $kind -ReadyEvent $readyNames[0] -ReleaseEvent $releaseName
                    $mode = if ($failSecond) { 'PrimaryFail' } else { 'Success' }
                    $b = Start-Worker $case $kind $mode -ReadyEvent $readyNames[1] -ReleaseEvent $releaseName
                    # Sequential event waits work in Windows PowerShell's STA and
                    # do not release either worker until BOTH have signaled.
                    $bothReady = $readyA.WaitOne(20000) -and $readyB.WaitOne(20000)
                    [void]$release.Set()
                    $ra = Complete-Worker $a
                    $rb = Complete-Worker $b
                    Assert-Lifecycle $bothReady 'Workers did not signal readiness within the bounded wait'
                    Assert-Lifecycle ($ra.ReachedBarrier -and $rb.ReachedBarrier -and $ra.BarrierReleased -and $rb.BarrierReleased) 'Both workers must overlap inside actual native-work call sites'
                    Assert-Lifecycle ($ra.OwnedTargets[0] -ine $rb.OwnedTargets[0]) 'Concurrent outputs reused a directory'
                    Assert-Lifecycle (-not $ra.Failed) $ra.Error
                    Assert-CompleteBackup $ra
                    if ($failSecond) {
                        Assert-Failure $rb
                        Assert-Lifecycle $rb.PrimarySeen 'Second worker failure injection missed'
                        Assert-OwnOutputRemoved $rb
                    } else {
                        Assert-Lifecycle (-not $rb.Failed) $rb.Error
                        Assert-CompleteBackup $rb
                    }
                    Assert-Sibling $case
                    Assert-CleanWorker $ra
                    Assert-CleanWorker $rb
                } finally {
                    [void]$release.Set()
                    $readyA.Dispose(); $readyB.Dispose(); $release.Dispose()
                }
            }
        }
    }

    foreach ($mode in @('Preexisting', 'QueryFail', 'MalformedOid', 'CreateRace')) {
        Check "restore $mode refuses without restore or drop" {
            $case = New-Case
            $db = 'tira_restore_case_' + [guid]::NewGuid().ToString('N')
            $r = Run-Worker $case 'restore' $mode -ScratchDb $db
            Assert-Failure $r
            Assert-Lifecycle (@(Events $r 'oid').Count -eq 1) 'Expected one pre-create catalog query'
            Assert-NoDrop $r
            Assert-Lifecycle (@(Events $r 'restore').Count -eq 0) 'Refused database was restored into'
            $expectedCreates = if ($mode -eq 'CreateRace') { 1 } else { 0 }
            Assert-Lifecycle (@(Events $r 'create').Count -eq $expectedCreates) 'Unexpected create attempt count'
            Assert-Lifecycle (-not $r.Created) 'Refusal/collision incorrectly acquired database ownership'
            if ($mode -eq 'Preexisting') {
                $entry = Get-CatalogEntry $r $db
                Assert-Lifecycle ($null -ne $entry -and $entry.Oid -ceq '700' -and $entry.Bytes -ceq 'preexisting-sentinel') 'Preexisting fake database was modified'
            } elseif ($mode -eq 'CreateRace') {
                $entry = Get-CatalogEntry $r $db
                Assert-Lifecycle ($null -ne $entry -and $entry.Oid -ceq '702' -and $entry.Bytes -ceq 'foreign-race-sentinel') 'Concurrent creator lost its database'
            }
            Assert-ForeignDatabase $r
            Assert-Sibling $case
        }
    }
    foreach ($mode in @('RestoreFail', 'BlobVerifyFail', 'CaptureQueryFail', 'CaptureBlankOid')) {
        Check "restore $mode retains owned database and materialized work" {
            $case = New-Case
            $db = 'tira_restore_case_' + [guid]::NewGuid().ToString('N')
            $r = Run-Worker $case 'restore' $mode -ScratchDb $db
            Assert-Failure $r
            Assert-Lifecycle $r.Created 'Fixture did not reach successful createdb'
            Assert-NoDrop $r
            $entry = Get-CatalogEntry $r $db
            Assert-Lifecycle ($null -ne $entry -and $entry.Oid -ceq '701' -and $entry.Owner -ceq 'worker') 'Failure discarded its created database'
            $expectedRestores = if ($mode -like 'Capture*') { 0 } else { 1 }
            Assert-Lifecycle (@(Events $r 'restore').Count -eq $expectedRestores) 'Restore ran before valid OID capture, or failed too early'
            Assert-KeptWork $r -Extracted:($mode -eq 'BlobVerifyFail')
            Assert-ForeignDatabase $r
        }
    }
    foreach ($mode in @('RolePreflightFail','QuarantineFail','AccessFail')) {
        Check "restore $mode cannot print success or drop its scratch resources" {
            $case = New-Case
            $r = Run-Worker $case 'restore' $mode -ScratchDb ('tira_restore_case_' + [guid]::NewGuid().ToString('N'))
            Assert-Failure $r
            Assert-NoDrop $r
            Assert-ForeignDatabase $r
            Assert-Lifecycle ($r.Error -match 'INJECT') 'Access failure injection was not reached'
            $expectedRestores = if ($mode -eq 'AccessFail') { 1 } else { 0 }
            Assert-Lifecycle (@(Events $r 'restore').Count -eq $expectedRestores) 'Access guard ordering is wrong'
            if ($mode -eq 'RolePreflightFail') { Assert-Lifecycle (-not $r.Created) 'Role preflight must precede database creation' }
            else { Assert-KeptWork $r }
        }
    }
    Check 'restore successful drill drops only its OID-owned database' {
        $case = New-Case
        $r = Run-Worker $case 'restore' -ScratchDb ('tira_restore_case_' + [guid]::NewGuid().ToString('N'))
        Assert-SuccessfulDrop $r
        Assert-Sibling $case
    }
    Check 'restore Keep preserves owned database and extracted work without any drop' {
        $case = New-Case
        $r = Run-Worker $case 'restore' -ScratchDb ('tira_restore_case_' + [guid]::NewGuid().ToString('N')) -Keep
        Assert-Lifecycle (-not $r.Failed) $r.Error
        Assert-NoDrop $r
        $entry = Get-CatalogEntry $r $r.Scratch
        Assert-Lifecycle ($null -ne $entry -and $entry.Oid -ceq '701') 'Keep discarded its database'
        Assert-Lifecycle (@(Events $r 'restore').Count -eq 1) 'Keep did not execute restore'
        Assert-KeptWork $r -Extracted
        Assert-ForeignDatabase $r
    }
    Check 'restore changed OID prevents drop of a replacement database' {
        $case = New-Case
        $r = Run-Worker $case 'restore' 'ChangedOid' -ScratchDb ('tira_restore_case_' + [guid]::NewGuid().ToString('N'))
        Assert-Failure $r
        Assert-Lifecycle (@(Events $r 'replaced').Count -eq 1) 'OID replacement injection not reached'
        Assert-Lifecycle (@(Events $r 'oid').Count -ge 3) 'Did not re-query ownership before drop'
        Assert-NoDrop $r
        $entry = Get-CatalogEntry $r $r.Scratch
        Assert-Lifecycle ($null -ne $entry -and $entry.Oid -ceq '703' -and $entry.Bytes -ceq 'foreign-replacement-sentinel') 'Replacement database was modified'
        Assert-ForeignDatabase $r
    }
    Check 'restore drop failure is not a successfully completed drill' {
        $case = New-Case
        $r = Run-Worker $case 'restore' 'DropFail' -ScratchDb ('tira_restore_case_' + [guid]::NewGuid().ToString('N'))
        Assert-Failure $r
        $drops = @(Events $r 'drop')
        Assert-Lifecycle ($drops.Count -eq 1 -and $drops[0].Oid -ceq '701') 'Drop failure did not occur on the owned database only'
        Assert-Lifecycle ($r.Error -match 'INJECT dropdb failure') 'Failed drop was not propagated'
        Assert-Lifecycle ($null -ne (Get-CatalogEntry $r $r.Scratch)) 'Failed drop should leave the synthetic database'
        Assert-ForeignDatabase $r
    }
    Check 'restore work cleanup error fails without a success banner after owned database drop' {
        $case = New-Case
        $r = Run-Worker $case 'restore' 'WorkCleanupError' -ScratchDb ('tira_restore_case_' + [guid]::NewGuid().ToString('N'))
        Assert-Failure $r
        $drops = @(Events $r 'drop')
        Assert-Lifecycle ($drops.Count -eq 1 -and $drops[0].Oid -ceq '701') 'Cleanup error must follow the owned database drop'
        Assert-Lifecycle ($null -eq (Get-CatalogEntry $r $r.Scratch)) 'Successfully dropped database should be absent'
        Assert-Lifecycle ($r.Error -match 'INJECT work cleanup error') 'Work cleanup error was not propagated'
        Assert-KeptWork $r -Extracted
        Assert-ForeignDatabase $r
    }
    Check 'restore default scratch names are unique lowercase GUID names' {
        $case = New-Case
        $a = Run-Worker $case 'restore'
        $b = Run-Worker $case 'restore'
        foreach ($r in @($a, $b)) {
            Assert-SuccessfulDrop $r
            Assert-Lifecycle ($r.Scratch -cmatch '^tira_restore_[0-9a-f]{32}$' -and $r.Scratch.Length -le 63) 'Default is not a lowercase GUID scratch name'
        }
        Assert-Lifecycle ($a.Scratch -cne $b.Scratch) 'Default database names were reused across workers'
    }
    $badNames = @('TIRA_RESTORE_upper', 'tira_restore_Mixed', ('tira_restore_' + ('a' * 51)),
                  'tira_restore_bad-name', "tira_restore_bad';--", ('tira_restore_' + [char]0x00e9),
                  "tira_restore_bad`n", "tira_restore_bad`r", 'tira_restore_')
    foreach ($badName in $badNames) {
        $displayName = $badName.Replace("`r", '\r').Replace("`n", '\n')
        Check "restore rejects unsafe scratch name [$displayName] before native work" {
            $case = New-Case
            $r = Run-Worker $case 'restore' -ScratchDb $badName
            Assert-Failure $r
            Assert-Lifecycle (@(Events $r 'native').Count -eq 0) 'Invalid name reached native/database dispatch'
            Assert-ForeignDatabase $r
        }
    }
    Check 'restore accepts a lowercase ASCII scratch name at the 63-character limit' {
        $case = New-Case
        $r = Run-Worker $case 'restore' -ScratchDb ('tira_restore_' + ('a' * 50))
        Assert-SuccessfulDrop $r
    }
} finally {
    # Never clean a fixture tree while a child might still be using it.
    foreach ($handle in $script:activeWorkers) {
        if (-not $handle.Finished) {
            try {
                if (-not $handle.Process.HasExited) {
                    $handle.Process.Kill()
                    Assert-Lifecycle ($handle.Process.WaitForExit(5000)) 'Could not stop the test child before fixture cleanup'
                }
            } finally { $handle.Process.Dispose() }
        }
    }
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop
}

Write-Host ("backup-lifecycle simulation: {0} passed, {1} failed" -f $script:passes, $script:failures)
if ($script:failures -gt 0) { exit 1 }
Write-Host 'BACKUP LIFECYCLE SIMULATION: PASS (not a real DB/engine proof)' -ForegroundColor Green
exit 0
