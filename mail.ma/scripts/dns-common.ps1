#requires -Version 5.1
<#
    Tira Mail - pure DNS-zone validation helpers (no Stalwart, no I/O).

    Why a separate library
    ----------------------
    dns-zone.ps1 talks to a live engine in its main flow, so it cannot be
    dot-sourced for tests. The sanity logic that does NOT need the engine lives
    here, where scripts/tests/dns-zone.Tests.ps1 can exercise it directly over
    fixed zone text. New offline DNS validators belong in this file.

    Dot-source it:
        . (Join-Path $PSScriptRoot "dns-common.ps1")
#>

function Get-MailHostFinding {
    <#
        Target-sanity check for a zone's mail host. Returns a finding object
        (level / code / message) when the MX target is NOT the host this domain
        is meant to use, or $null when it is sane.

        A single Stalwart engine can legitimately serve many domains from one
        shared host (every domain's MX points at mail.tira.test). That is valid,
        but it must be VISIBLE: exporting uit.ac.ma with a borrowed mail.tira.test
        host and reporting a bare "ok" hides that this is a shared/demo
        delegation, not a self-hosted production zone. So an out-of-domain host
        is a warning, never a silent pass.

        -ExpectedHost (from dns-zone.ps1 -MailHost) pins the intended host; the
        actual MX target must equal it. Without it, the host is simply expected
        to live under the exported domain (apex or a subdomain).

        Parameters are plain strings so the function is pure and testable:
        -MxText is one MX record line, e.g. "uit.ac.ma. IN MX 10 mail.tira.test."
    #>
    param(
        [Parameter(Mandatory)] [string] $MxText,
        [Parameter(Mandatory)] [string] $DomainName,
        [string] $ExpectedHost
    )

    # An MX record is "<owner> IN MX <preference> <target>."; read the target
    # positionally after the MX keyword so a malformed record (no preference or
    # no target) yields $null instead of inventing a host from a stray token.
    $tokens = @($MxText -split '\s+' | Where-Object { $_ -ne "" })
    $mxIndex = -1
    for ($i = 0; $i -lt $tokens.Count; $i++) {
        if ($tokens[$i].Equals("MX", [System.StringComparison]::OrdinalIgnoreCase)) { $mxIndex = $i; break }
    }
    if ($mxIndex -lt 0 -or ($mxIndex + 2) -ge $tokens.Count) {
        # No MX keyword, or no preference+target after it: the presence check
        # owns this, not us.
        return $null
    }
    $target = $tokens[$mxIndex + 2].TrimEnd('.').ToLowerInvariant()
    $domain = $DomainName.TrimEnd('.').ToLowerInvariant()
    $inDomain = ($target -eq $domain) -or $target.EndsWith(".$domain")

    if (-not [string]::IsNullOrWhiteSpace($ExpectedHost)) {
        $expected = $ExpectedHost.TrimEnd('.').ToLowerInvariant()
        if ($target -ne $expected) {
            return [pscustomobject]@{
                level   = "warn"
                code    = "mail-host-mismatch"
                message = "MX host '$target' does not match the requested -MailHost '$expected'"
            }
        }
        return $null
    }

    if (-not $inDomain) {
        return [pscustomobject]@{
            level   = "warn"
            code    = "mail-host-external"
            message = "MX host '$target' is outside '$domain' (a shared/borrowed engine host, expected mail.$domain). Confirm this is an intentional multi-domain or demo delegation before publishing."
        }
    }

    return $null
}