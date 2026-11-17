#requires -Version 5.1
<#
    Tira Mail - SMTP end-to-end proof against Stalwart.

    Proves:
      1. Inbound MTA path: an unauthenticated peer delivers mail to a local mailbox
      2. STARTTLS on the SMTP port
      3. Authenticated submission (AUTH PLAIN) from the account itself
      4. Both messages are visible over IMAP afterwards

    Usage (the password is never hardcoded: supply it via TIRA_PROOF_PASSWORD or
    -Password, and opt in to the self-signed test certificate explicitly):
      $env:TIRA_PROOF_PASSWORD = '...'
      .\scripts\smtp-proof.ps1 -SkipCertificateCheck

    Without -SkipCertificateCheck the system validates the certificate chain and
    hostname, so a self-signed test certificate is rejected (fail closed). The
    opt-in is refused for any non-loopback -HostName.
#>
[CmdletBinding()]
param(
    [string] $HostName = "127.0.0.1",
    [int]    $InboundPort = 25,
    [int]    $SubmissionPort = 2526,
    [string] $ServerName = "mail.tira.test",
    [string] $User = "yassine@tira.test",
    [string] $Password,
    [int]    $ImapPort = 1143,
    [switch] $SkipCertificateCheck
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "proof-common.ps1")

# Fail closed before any network I/O: no hardcoded password fallback.
$Password = Resolve-ProofPassword -Explicit $Password -EnvironmentVariable "TIRA_PROOF_PASSWORD" -Label "SMTP"

function New-TextConnection {
    param([System.IO.Stream] $Stream)
    $reader = New-Object System.IO.StreamReader($Stream, [System.Text.Encoding]::ASCII, $false, 1)
    $writer = New-Object System.IO.StreamWriter($Stream, [System.Text.Encoding]::ASCII, 1)
    $writer.NewLine = "`r`n"
    $writer.AutoFlush = $true
    [pscustomobject]@{ Reader = $reader; Writer = $writer }
}

function Read-SmtpReply {
    param([Parameter(Mandatory)] [System.IO.StreamReader] $Reader)
    $lines = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $line = $Reader.ReadLine()
        if ($null -eq $line) { throw "SMTP connection closed unexpectedly." }
        $lines.Add($line)
        if ($line.Length -ge 4 -and $line[3] -eq ' ') { return $lines }
        if ($line.Length -eq 3) { return $lines }
    }
}

function Assert-Code {
    param([string[]] $Reply, [string] $Code, [string] $Label)
    $first = $Reply[0]
    if (-not $first.StartsWith($Code)) {
        throw "ASSERT FAILED [$Label]: expected $Code, got:`n  $($Reply -join "`n  ")"
    }
    Write-Host ("  ok  {0} -> {1}" -f $Label, $first) -ForegroundColor Green
}

function Send-SmtpMessage {
    param(
        [Parameter(Mandatory)] $Conn,
        [Parameter(Mandatory)] [string] $From,
        [Parameter(Mandatory)] [string] $To,
        [Parameter(Mandatory)] [string] $Message,
        [string] $AuthLine
    )
    $Conn.Writer.WriteLine("EHLO tira.test")
    $ehlo = Read-SmtpReply -Reader $Conn.Reader
    Assert-Code -Reply $ehlo -Code "250" -Label "EHLO"
    $ehlo | Where-Object { $_ -match "^(250-|250 )" } | ForEach-Object { Write-Host "    $_" }

    if ($AuthLine) {
        $Conn.Writer.WriteLine($AuthLine)
        $auth = Read-SmtpReply -Reader $Conn.Reader
        Assert-Code -Reply $auth -Code "235" -Label "AUTH PLAIN"
    }

    $Conn.Writer.WriteLine("MAIL FROM:<$From>")
    Assert-Code -Reply (Read-SmtpReply -Reader $Conn.Reader) -Code "250" -Label "MAIL FROM"

    $Conn.Writer.WriteLine("RCPT TO:<$To>")
    Assert-Code -Reply (Read-SmtpReply -Reader $Conn.Reader) -Code "250" -Label "RCPT TO"

    $Conn.Writer.WriteLine("DATA")
    $data = Read-SmtpReply -Reader $Conn.Reader
    Assert-Code -Reply $data -Code "354" -Label "DATA"

    foreach ($line in ($Message -split "`r`n")) {
        if ($line.StartsWith(".")) { $Conn.Writer.WriteLine(".$line") } else { $Conn.Writer.WriteLine($line) }
    }
    $Conn.Writer.WriteLine(".")
    Assert-Code -Reply (Read-SmtpReply -Reader $Conn.Reader) -Code "250" -Label "message accepted"

    $Conn.Writer.WriteLine("QUIT")
    [void](Read-SmtpReply -Reader $Conn.Reader)
}

function New-Message {
    param([string] $From, [string] $To, [string] $Tag)
    $id = "<smtp-$Tag-$([guid]::NewGuid().ToString('N'))@tira.test>"
    $lines = @(
        "From: $From"
        "To: $To"
        "Subject: SMTP proof $Tag"
        "Date: $([DateTime]::UtcNow.ToString('r'))"
        "Message-ID: $id"
        "MIME-Version: 1.0"
        "Content-Type: text/plain; charset=us-ascii"
        ""
        "Message $Tag injected over SMTP."
    )
    [pscustomobject]@{ Text = ($lines -join "`r`n"); MessageId = $id }
}

# The TLS validation callback comes from proof-common.ps1 (New-ProofSslStream):
# system validation by default, an explicit loopback-only opt-in otherwise.

# ------------------------------------------------- 1. inbound, unauthenticated
Write-Host "`n[1] Inbound MTA delivery (no auth) to ${HostName}:${InboundPort}" -ForegroundColor Cyan
$inbound = New-Message -From "sender@external.test" -To $User -Tag "inbound"
$tcp = New-Object System.Net.Sockets.TcpClient
$tcp.Connect($HostName, $InboundPort)
$conn = New-TextConnection -Stream $tcp.GetStream()
Assert-Code -Reply (Read-SmtpReply -Reader $conn.Reader) -Code "220" -Label "greeting"
Send-SmtpMessage -Conn $conn -From "sender@external.test" -To $User -Message $inbound.Text
$tcp.Close()
Write-Host "  message-id: $($inbound.MessageId)"

# --------------------------------------------------------- 2. STARTTLS + AUTH
Write-Host "`n[2] STARTTLS + AUTH PLAIN on ${HostName}:${SubmissionPort}" -ForegroundColor Cyan
$tcp2 = New-Object System.Net.Sockets.TcpClient
$tcp2.Connect($HostName, $SubmissionPort)
$net2 = $tcp2.GetStream()
$plain = New-TextConnection -Stream $net2
Assert-Code -Reply (Read-SmtpReply -Reader $plain.Reader) -Code "220" -Label "greeting"

$plain.Writer.WriteLine("EHLO tira.test")
$ehlo1 = Read-SmtpReply -Reader $plain.Reader
Assert-Code -Reply $ehlo1 -Code "250" -Label "EHLO (cleartext)"
if (-not ($ehlo1 -match "STARTTLS")) { throw "ASSERT FAILED: server does not advertise STARTTLS." }

$plain.Writer.WriteLine("STARTTLS")
Assert-Code -Reply (Read-SmtpReply -Reader $plain.Reader) -Code "220" -Label "STARTTLS"

$ssl = New-ProofSslStream -Stream $net2 -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$ssl.AuthenticateAsClient($ServerName)
Write-Host ("  tls: {0} / cipher {1}" -f $ssl.SslProtocol, $ssl.CipherAlgorithm)
$secure = New-TextConnection -Stream $ssl

$submitted = New-Message -From $User -To $User -Tag "submission"
$authPlain = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("`0$User`0$Password"))
Send-SmtpMessage -Conn $secure -From $User -To $User -Message $submitted.Text -AuthLine "AUTH PLAIN $authPlain"
$ssl.Dispose()
$tcp2.Close()
Write-Host "  message-id: $($submitted.MessageId)"

# ------------------------------------------------------- 3. verify over IMAP
Write-Host "`n[3] Verify both messages arrived, over IMAP STARTTLS" -ForegroundColor Cyan
$tcp3 = New-Object System.Net.Sockets.TcpClient
$tcp3.Connect($HostName, $ImapPort)
$net3 = $tcp3.GetStream()
$imapPlain = New-TextConnection -Stream $net3
[void]$imapPlain.Reader.ReadLine()
$imapPlain.Writer.WriteLine("c1 STARTTLS")
$reply = @()
while ($true) { $l = $imapPlain.Reader.ReadLine(); $reply += $l; if ($l -match "^c1 ") { break } }
if (-not ($reply -match "^c1 OK")) { throw "IMAP STARTTLS failed: $($reply -join ' | ')" }
$imapSsl = New-ProofSslStream -Stream $net3 -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$imapSsl.AuthenticateAsClient($ServerName)
$imap = New-TextConnection -Stream $imapSsl
$imap.Writer.WriteLine("c2 LOGIN $User $Password")
$login = @()
while ($true) { $l = $imap.Reader.ReadLine(); $login += $l; if ($l -match "^c2 ") { break } }
if (-not ($login -match "^c2 OK")) { throw "IMAP LOGIN failed: $($login -join ' | ')" }
Write-Host "  ok  IMAP LOGIN" -ForegroundColor Green

$deadline = (Get-Date).AddSeconds(20)
$foundInbound = $null
$foundSubmitted = $null
$tag = 10
while ((Get-Date) -lt $deadline) {
    foreach ($box in @("INBOX", "Junk Mail")) {
        $tag++
        $imap.Writer.WriteLine("c$tag SELECT `"$box`"")
        $sel = @()
        while ($true) { $l = $imap.Reader.ReadLine(); $sel += $l; if ($l -match "^c$tag ") { break } }
        $tag++
        $imap.Writer.WriteLine("c$tag FETCH 1:* (BODY.PEEK[HEADER.FIELDS (MESSAGE-ID SUBJECT)])")
        $fetch = @()
        while ($true) { $l = $imap.Reader.ReadLine(); $fetch += $l; if ($l -match "^c$tag ") { break } }
        $joined = $fetch -join "`n"
        if (-not $foundInbound -and $joined -match [regex]::Escape($inbound.MessageId)) { $foundInbound = $box }
        if (-not $foundSubmitted -and $joined -match [regex]::Escape($submitted.MessageId)) { $foundSubmitted = $box }
    }
    if ($foundInbound -and $foundSubmitted) { break }
    Start-Sleep -Seconds 2
}

$imap.Writer.WriteLine("c99 LOGOUT")
[void]$imap.Reader.ReadLine()
$imapSsl.Dispose()
$tcp3.Close()

if (-not $foundInbound) { throw "ASSERT FAILED: the inbound (unauthenticated) message never appeared." }
Write-Host ("  ok  inbound message delivered to: {0}" -f $foundInbound) -ForegroundColor Green
if (-not $foundSubmitted) { throw "ASSERT FAILED: the authenticated submission never appeared." }
Write-Host ("  ok  submitted message delivered to: {0}" -f $foundSubmitted) -ForegroundColor Green
if ($foundSubmitted -ne "INBOX") {
    Write-Warning "The submitted message was filed under '$foundSubmitted' - expected until SPF/DKIM/DMARC exist for tira.test (phase P3)."
}

Write-Host "`nSMTP proof: PASS" -ForegroundColor Green
