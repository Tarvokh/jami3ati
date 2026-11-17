#requires -Version 5.1
<#
    Tira Mail - export and validate the DNS zone Stalwart generated for a domain.

    Why this exists
    ---------------
    The engine already creates the records that make outbound mail deliverable:
    SPF, DKIM (RSA-2048 + Ed25519), DMARC p=reject, MTA-STS, TLS-RPT, MX and
    autoconfig. Those records live in the domain object (`dnsZoneFile`) but are
    useless until someone publishes them. This script turns them into a normal
    zone file and refuses to hand back a zone that is missing a control.

    It also checks the actual key strength, not just the presence of a TXT
    record: an RSA `p=` value is decoded from SubjectPublicKeyInfo and its
    modulus bit length must be >= 2048. A 1024-bit key published by mistake is
    the kind of thing that only shows up after mail starts landing in Junk.

    Usage
    -----
        $env:STALWART_URL  = "http://127.0.0.1:8080"
        $env:STALWART_USER = "admin@tira.test"
        $env:STALWART_PASSWORD = "..."

        .\scripts\dns-zone.ps1                       # all domains -> .\deploy\dns
        .\scripts\dns-zone.ps1 -Domain tira.test
        .\scripts\dns-zone.ps1 -OutDir D:\dns -Strict # warnings become failures
        .\scripts\dns-zone.ps1 -Domain uit.ac.ma -MailHost mail.uit.ac.ma

    Mail-host sanity
    ----------------
    The MX target is checked against the domain it serves. A single engine can
    legitimately serve many domains from one shared host (every demo domain's
    MX points at mail.tira.test); that is reported as a warning, never a silent
    "ok", so a borrowed-host demo zone is not mistaken for a self-hosted
    production zone. `-MailHost` pins the intended host: the actual MX target
    must equal it, otherwise the export warns.

    DMARC rollout
    -------------
    The engine hardcodes `p=reject`. That is the right end state, but it is
    reckless on day one for a domain that already sends mail from systems you
    do not control (a university almost always does). The staged rollout is:

        -DmarcPolicy none          # 14 days: collect reports, find senders
        -DmarcPolicy quarantine    # 14 days: suspicious mail to Junk
        -DmarcPolicy reject        # final: refuse outright

    `-DmarcPolicy` rewrites the `_dmarc` record in the exported zone (and in
    the JSON sidecar) so the file you publish matches the intended stage.
    `-DmarcPct 10` adds `pct=10` for a percentage rollout, and
    `-DmarcSubdomainPolicy` sets `sp=` for subdomains that still send mail.
    The default `engine` leaves Stalwart's record untouched.

    Exit code 0 only if every mandatory record is present and valid.
#>
[CmdletBinding()]
param(
    [string] $Domain,
    [string] $MailHost,
    [string] $OutDir,
    [string] $StalwartCli = "C:\tools\stalwart\stalwart-cli.exe",
    [ValidateSet("engine", "none", "quarantine", "reject")]
    [string] $DmarcPolicy = "engine",
    [ValidateSet("", "none", "quarantine", "reject")]
    [string] $DmarcSubdomainPolicy = "",
    [ValidateRange(1, 100)]
    [int] $DmarcPct = 100,
    [switch] $Strict
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# $PSScriptRoot is empty while param defaults are evaluated under -File, so
# resolve it here in the body instead of in the param() block.
$scriptRoot = if ([string]::IsNullOrWhiteSpace($PSScriptRoot)) { (Get-Location).Path } else { $PSScriptRoot }

# Pure, offline-testable DNS validators (no engine, no I/O) live next door.
. (Join-Path $scriptRoot "dns-common.ps1")

if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path $scriptRoot "..\deploy\dns"
}

function Get-StalwartJson {
    param([string[]] $Arguments)
    if (-not (Test-Path -LiteralPath $StalwartCli)) {
        throw "stalwart-cli not found: $StalwartCli"
    }
    $output = & $StalwartCli @Arguments --json 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "stalwart-cli $($Arguments -join ' ') failed: $($output -join ' ')"
    }
    $jsonLines = New-Object System.Collections.Generic.List[string]
    foreach ($line in $output) {
        $text = ([string]$line).Trim()
        if ($text.StartsWith("{") -or $text.StartsWith("[")) {
            $jsonLines.Add($text)
        }
    }
    if ($jsonLines.Count -eq 0) { return $null }
    return ($jsonLines -join "`n")
}

# ---------------------------------------------------------------- DER helpers ---

function Read-DerLength {
    param([byte[]] $Bytes, [ref] $Offset)
    $first = $Bytes[$Offset.Value]
    $Offset.Value++
    if ($first -lt 0x80) { return $first }
    $count = $first -band 0x7F
    if ($count -eq 0) { throw "indefinite DER length is not valid here" }
    $length = 0
    for ($i = 0; $i -lt $count; $i++) {
        $length = ($length -shl 8) -bor $Bytes[$Offset.Value]
        $Offset.Value++
    }
    return $length
}

function Get-RsaModulusBits {
    param([Parameter(Mandatory)] [string] $Base64Spki)
    $der = [Convert]::FromBase64String($Base64Spki)
    $offset = 0

    # SubjectPublicKeyInfo ::= SEQUENCE { algorithm SEQUENCE, subjectPublicKey BIT STRING }
    if ($der[$offset] -ne 0x30) { throw "SPKI is not a SEQUENCE" }
    $offset++
    [void](Read-DerLength -Bytes $der -Offset ([ref]$offset))

    if ($der[$offset] -ne 0x30) { throw "AlgorithmIdentifier is not a SEQUENCE" }
    $offset++
    $algoLength = Read-DerLength -Bytes $der -Offset ([ref]$offset)
    $offset += $algoLength

    if ($der[$offset] -ne 0x03) { throw "subjectPublicKey is not a BIT STRING" }
    $offset++
    $bitLength = Read-DerLength -Bytes $der -Offset ([ref]$offset)
    $end = $offset + $bitLength
    $offset++ # number of unused bits

    # RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
    if ($der[$offset] -ne 0x30) { throw "RSAPublicKey is not a SEQUENCE" }
    $offset++
    [void](Read-DerLength -Bytes $der -Offset ([ref]$offset))
    if ($der[$offset] -ne 0x02) { throw "RSA modulus is not an INTEGER" }
    $offset++
    $modulusLength = Read-DerLength -Bytes $der -Offset ([ref]$offset)
    $modulusStart = $offset
    $modulusEnd = $modulusStart + $modulusLength
    if ($modulusEnd -gt $end) { throw "RSA modulus runs past the BIT STRING" }

    $leadingZeros = 0
    while ($leadingZeros -lt $modulusLength -and $der[$modulusStart + $leadingZeros] -eq 0) {
        $leadingZeros++
    }
    $significant = $modulusLength - $leadingZeros
    if ($significant -le 0) { return 0 }

    $top = $der[$modulusStart + $leadingZeros]
    $bits = ($significant - 1) * 8
    while ($top -gt 0) {
        $bits++
        $top = $top -shr 1
    }
    return $bits
}

# ------------------------------------------------------------- zone validation ---

function Get-ZoneRecords {
    param([Parameter(Mandatory)] [string] $ZoneText)
    $records = New-Object System.Collections.Generic.List[object]
    $buffer = ""
    foreach ($rawLine in ($ZoneText -split "`r?`n")) {
        $line = $rawLine.Trim()
        if ($line.Length -eq 0 -or $line.StartsWith(";")) { continue }
        if ($line.StartsWith("(")) { continue }
        if ($line -eq ")") {
            if ($buffer.Length -gt 0) {
                $records.Add([pscustomobject]@{ Text = $buffer.Trim(); Buffer = $true })
                $buffer = ""
            }
            continue
        }
        if ($line.EndsWith("(")) {
            $buffer = $line.TrimEnd("(").Trim()
            continue
        }
        if ($buffer.Length -gt 0) {
            $buffer = $buffer + " " + $line.TrimEnd(")").Trim()
            if ($line.EndsWith(")")) {
                $records.Add([pscustomobject]@{ Text = $buffer.Trim(); Buffer = $true })
                $buffer = ""
            }
            continue
        }
        $records.Add([pscustomobject]@{ Text = $line; Buffer = $false })
    }
    return $records
}

function Find-ZoneRecord {
    param($Records, [string] $OwnerPattern, [string] $Type, [string] $ValuePattern)
    return $Records | Where-Object {
        $_.Text -match $OwnerPattern -and
        $_.Text -cmatch "\s$Type\s" -and
        ($ValuePattern -eq "" -or $_.Text -match $ValuePattern)
    } | Select-Object -First 1
}

function Get-TxtValue {
    param([string] $RecordText)
    $quoted = [regex]::Matches($RecordText, '"([^"]*)"')
    if ($quoted.Count -gt 0) {
        return (($quoted | ForEach-Object { $_.Groups[1].Value }) -join "")
    }
    return $RecordText
}

function Set-DmarcPolicy {
    <#
        Rewrites the `_dmarc.<domain>` TXT record in a zone file. Returns $null
        when the record is absent or not on a single line, so the caller can
        report a hard failure instead of silently exporting the wrong policy.

        Stalwart emits the record on one line:
            _dmarc.example.com. IN TXT "v=DMARC1; p=reject; rua=mailto:..."
        Anything more exotic is treated as "cannot rewrite" on purpose: guessing
        inside a multi-line record is how you publish `p=none` by accident.
    #>
    param(
        [Parameter(Mandatory)] [string] $ZoneText,
        [Parameter(Mandatory)] [string] $DomainName,
        [Parameter(Mandatory)] [string] $Policy,
        [string] $SubdomainPolicy,
        [int] $Percent
    )

    $pattern = '(?m)^(?<owner>_dmarc\.' + [regex]::Escape($DomainName) +
        '\.\s+IN\s+TXT\s+")(?<value>v=DMARC1;[^"]*)(?<close>")\s*$'
    $match = [regex]::Match($ZoneText, $pattern)
    if (-not $match.Success) { return $null }

    $original = $match.Groups['value'].Value
    $tags = @("v=DMARC1", "p=$Policy")
    if ($SubdomainPolicy) { $tags += "sp=$SubdomainPolicy" }
    if ($Percent -lt 100) { $tags += "pct=$Percent" }

    # Preserve the reporting and alignment tags Stalwart generated. Dropping
    # rua would quietly stop the reports that make a staged rollout visible.
    foreach ($key in @("rua", "ruf", "adkim", "aspf", "fo", "rf", "ri")) {
        $tag = [regex]::Match($original, "(?:^|;\s*)$key=([^;]+)")
        if ($tag.Success) { $tags += "$key=$($tag.Groups[1].Value.Trim())" }
    }

    $replacement = $match.Groups['owner'].Value + ($tags -join "; ") + $match.Groups['close'].Value
    return [pscustomobject]@{
        ZoneText = $ZoneText.Substring(0, $match.Index) + $replacement +
            $ZoneText.Substring($match.Index + $match.Length)
        Value    = ($tags -join "; ")
    }
}

function Add-Finding {
    param(
        [System.Collections.Generic.List[object]] $Findings,
        [string] $Level,
        [string] $Code,
        [string] $Message
    )
    $Findings.Add([pscustomobject]@{ level = $Level; code = $Code; message = $Message })
}

# ------------------------------------------------------------------- main flow ---

$domainRows = Get-StalwartJson @("query", "Domain", "--fields", "id,name,dnsZoneFile")
if (-not $domainRows) { throw "no domains returned by Stalwart" }
$domains = @($domainRows -split "`n" | ForEach-Object { $_ | ConvertFrom-Json })

$keyRows = Get-StalwartJson @("query", "DkimSignature", "--fields", "domainId,selector,stage,publicKey,@type")
$keys = @()
if ($keyRows) {
    $keys = @($keyRows -split "`n" | ForEach-Object { $_ | ConvertFrom-Json })
}

$domainNames = @{}
foreach ($row in $domains) { $domainNames[$row.id] = $row.name }

$selected = $domains
if ($Domain) {
    $selected = @($domains | Where-Object { $_.name -eq $Domain })
    if ($selected.Count -eq 0) { throw "domain not found in Stalwart: $Domain" }
}

$OutDir = [System.IO.Path]::GetFullPath($OutDir)
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# PowerShell 5.1's `Set-Content -Encoding UTF8` emits a BOM, which JSON
# consumers (and the Rust proof endpoint) correctly treat as data, not as
# whitespace. Write UTF-8 without a BOM so exported artifacts are portable.
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

$failed = $false
$summary = New-Object System.Collections.Generic.List[object]

foreach ($row in $selected) {
    $name = $row.name
    Write-Host "`n[$name]" -ForegroundColor Cyan
    $findings = New-Object System.Collections.Generic.List[object]
    $zoneText = if ($null -ne $row.dnsZoneFile) { [string]$row.dnsZoneFile } else { "" }

    if (-not [string]::IsNullOrWhiteSpace($zoneText) -and $DmarcPolicy -ne "engine") {
        $rewritten = Set-DmarcPolicy -ZoneText $zoneText -DomainName $name `
            -Policy $DmarcPolicy -SubdomainPolicy $DmarcSubdomainPolicy -Percent $DmarcPct
        if ($null -eq $rewritten) {
            Add-Finding $findings "error" "dmarc-rewrite-failed" `
                "could not rewrite the _dmarc record; refusing to export Stalwart's policy"
        } else {
            $zoneText = $rewritten.ZoneText
            Add-Finding $findings "ok" "dmarc-staged" "exported DMARC: $($rewritten.Value)"
        }
    }

    if ([string]::IsNullOrWhiteSpace($zoneText)) {
        Add-Finding $findings "error" "zone-empty" "Stalwart returned an empty dnsZoneFile"
    } else {
        $records = Get-ZoneRecords -ZoneText $zoneText

        $spf = Find-ZoneRecord $records "^$([regex]::Escape($name))\.\s" "TXT" "v=spf1"
        if (-not $spf) {
            Add-Finding $findings "error" "spf-missing" "no v=spf1 TXT record at the apex"
        } else {
            $value = Get-TxtValue $spf.Text
            if ($value -match '\+all') {
                Add-Finding $findings "error" "spf-open" "SPF uses +all (an open sender policy)"
            } elseif ($value -notmatch '[-~]all') {
                Add-Finding $findings "error" "spf-no-all" "SPF has no -all or ~all terminator"
            } else {
                Add-Finding $findings "ok" "spf" "SPF present: $value"
            }
        }

        $mx = Find-ZoneRecord $records "^$([regex]::Escape($name))\.\s" "MX" ""
        if (-not $mx) {
            Add-Finding $findings "error" "mx-missing" "no MX record at the apex"
        } else {
            Add-Finding $findings "ok" "mx" "MX present: $($mx.Text)"
            # Target sanity: a borrowed/shared mail host (every demo domain
            # pointing at mail.tira.test) is valid but must be visible, not a
            # silent "ok". -MailHost pins the intended host when the operator
            # knows it; otherwise an out-of-domain host is flagged.
            $hostFinding = Get-MailHostFinding -MxText $mx.Text -DomainName $name -ExpectedHost $MailHost
            if ($null -ne $hostFinding) {
                Add-Finding $findings $hostFinding.level $hostFinding.code $hostFinding.message
            }
        }

        $dmarc = Find-ZoneRecord $records "^_dmarc\.$([regex]::Escape($name))\.\s" "TXT" "v=DMARC1"
        if (-not $dmarc) {
            Add-Finding $findings "error" "dmarc-missing" "no _dmarc TXT record"
        } else {
            $value = Get-TxtValue $dmarc.Text
            $actual = [regex]::Match($value, '(?:^|;\s*)p=([a-z]+)').Groups[1].Value

            if ($DmarcPolicy -eq "none") {
                # Stage 1 is intentional: reports only, no enforcement. It is
                # reported as ok so `-Strict` does not undo the rollout plan.
                if ($actual -eq "none") {
                    Add-Finding $findings "ok" "dmarc-staged" "DMARC p=none (stage 1: monitoring)"
                } else {
                    Add-Finding $findings "warn" "dmarc-policy-mismatch" `
                        "zone says p=$actual but -DmarcPolicy none was requested"
                }
            } elseif ($DmarcPolicy -in @("quarantine", "reject")) {
                if ($actual -eq $DmarcPolicy) {
                    Add-Finding $findings "ok" "dmarc" "DMARC present: $value"
                } else {
                    Add-Finding $findings "error" "dmarc-policy-mismatch" `
                        "zone says p=$actual but -DmarcPolicy $DmarcPolicy was requested"
                }
            } elseif ($actual -eq "none") {
                Add-Finding $findings "error" "dmarc-none" "DMARC is p=none (monitor only)"
            } elseif ($actual -notmatch '^(reject|quarantine)$') {
                Add-Finding $findings "error" "dmarc-weak" "DMARC policy is not reject/quarantine"
            } else {
                Add-Finding $findings "ok" "dmarc" "DMARC present: $value"
            }

            if ($DmarcPct -lt 100) {
                $actualPct = [regex]::Match($value, '(?:^|;\s*)pct=(\d+)').Groups[1].Value
                if ($actualPct -ne [string]$DmarcPct) {
                    Add-Finding $findings "warn" "dmarc-pct-mismatch" `
                        "zone says pct=$actualPct but -DmarcPct $DmarcPct was requested"
                }
            }
        }

        $mtaSts = Find-ZoneRecord $records "^_mta-sts\.$([regex]::Escape($name))\.\s" "TXT" "v=STSv1"
        if (-not $mtaSts) {
            Add-Finding $findings "error" "mta-sts-missing" "no _mta-sts TXT record"
        } else {
            Add-Finding $findings "ok" "mta-sts" "MTA-STS present"
        }
        $mtaStsHost = Find-ZoneRecord $records "^mta-sts\.$([regex]::Escape($name))\.\s" "CNAME" ""
        if (-not $mtaStsHost) {
            Add-Finding $findings "error" "mta-sts-host-missing" "mta-sts.<domain> CNAME is missing"
        }

        $tlsRpt = Find-ZoneRecord $records "^_smtp\._tls\.$([regex]::Escape($name))\.\s" "TXT" "v=TLSRPTv1"
        if (-not $tlsRpt) {
            Add-Finding $findings "warn" "tls-rpt-missing" "no _smtp._tls TLS-RPT record"
        } else {
            Add-Finding $findings "ok" "tls-rpt" "TLS-RPT present"
        }

        $activeKeys = @($keys | Where-Object {
            $_.domainId -eq $row.id -and $_.stage -eq "active"
        })
        if ($activeKeys.Count -eq 0) {
            Add-Finding $findings "error" "dkim-missing" "no active DKIM signature for this domain"
        }
        foreach ($key in $activeKeys) {
            $owner = "^$([regex]::Escape($key.selector))\._domainkey\.$([regex]::Escape($name))\.\s"
            $dkim = Find-ZoneRecord $records $owner "TXT" "v=DKIM1"
            if (-not $dkim) {
                Add-Finding $findings "error" "dkim-record-missing" "no TXT record for selector $($key.selector)"
                continue
            }
            $value = Get-TxtValue $dkim.Text
            if ($value -notmatch 'p=[A-Za-z0-9+/=]+') {
                Add-Finding $findings "error" "dkim-no-key" "selector $($key.selector) has an empty p= value"
                continue
            }
            $p = [regex]::Match($value, 'p=([A-Za-z0-9+/=]+)').Groups[1].Value

            if ($key.'@type' -eq "Dkim1RsaSha256") {
                try {
                    $bits = Get-RsaModulusBits -Base64Spki $p
                    if ($bits -lt 2048) {
                        Add-Finding $findings "error" "dkim-rsa-weak" "selector $($key.selector): RSA $bits bit (< 2048)"
                    } else {
                        Add-Finding $findings "ok" "dkim-rsa" "selector $($key.selector): RSA $bits bit"
                    }
                } catch {
                    Add-Finding $findings "error" "dkim-rsa-parse" "selector $($key.selector): cannot parse p= ($($_.Exception.Message))"
                }
            } elseif ($key.'@type' -eq "Dkim1Ed25519Sha256") {
                $rawLength = [Convert]::FromBase64String($p).Length
                if ($rawLength -ne 32) {
                    Add-Finding $findings "error" "dkim-ed25519" "selector $($key.selector): Ed25519 key is $rawLength bytes (expected 32)"
                } else {
                    Add-Finding $findings "ok" "dkim-ed25519" "selector $($key.selector): Ed25519 32-byte key"
                }
            } else {
                Add-Finding $findings "warn" "dkim-type" "selector $($key.selector): unhandled type $($key.'@type')"
            }
        }
    }

    $zonePath = Join-Path $OutDir "$name.zone"
    [System.IO.File]::WriteAllText($zonePath, $zoneText, $utf8NoBom)

    $json = [pscustomobject]@{
        domain      = $name
        generatedAt = [DateTime]::UtcNow.ToString("o")
        # Portable provenance: the basename, relative to this JSON's own
        # directory. An absolute machine path leaks the dev layout and breaks
        # on any other host; the proof loads <domain>.zone by convention, not
        # from this field, so a basename is both honest and sufficient.
        zoneFile    = "$name.zone"
        dmarcPolicy = $DmarcPolicy
        records     = if ($zoneText) { (Get-ZoneRecords -ZoneText $zoneText).Count } else { 0 }
        activeKeys  = @($keys | Where-Object { $_.domainId -eq $row.id -and $_.stage -eq "active" } | ForEach-Object {
            [pscustomobject]@{ selector = $_.selector; type = $_.'@type' }
        })
        findings    = $findings
    }
    $jsonPath = Join-Path $OutDir "$name.dns.json"
    [System.IO.File]::WriteAllText($jsonPath, ($json | ConvertTo-Json -Depth 8), $utf8NoBom)
    $summary.Add($json)

    foreach ($finding in $findings) {
        $color = switch ($finding.level) {
            "ok" { "Green" }
            "warn" { "Yellow" }
            default { "Red" }
        }
        Write-Host ("  {0,-5} {1,-22} {2}" -f $finding.level, $finding.code, $finding.message) -ForegroundColor $color
    }
    $errors = @($findings | Where-Object { $_.level -eq "error" }).Count
    $warnings = @($findings | Where-Object { $_.level -eq "warn" }).Count
    if ($errors -gt 0 -or ($Strict -and $warnings -gt 0)) { $failed = $true }
    Write-Host ("  zone: {0}" -f $zonePath)
}

Write-Host ""
if ($failed) {
    Write-Host "DNS ZONE VALIDATION FAILED" -ForegroundColor Red
    exit 1
}
Write-Host ("DNS ZONE VALIDATION PASSED ({0} domain(s))" -f $selected.Count) -ForegroundColor Green
