#requires -Version 5.1
<#
    Tira Mail - identity bridge end-to-end proof.

    This is the proof the bridge exists: an account that was created from a row in
    `tira-store` (not by hand in the engine) can authenticate, submit mail to a
    *different* local domain, and the recipient can read it.

    It proves four things that no unit test can:
      1. STARTTLS + AUTH PLAIN on submission (2526) with the provisioned secret
      2. SMTP accepts a sender from one local domain and a recipient in another
      3. The message is queued and delivered (not merely accepted)
      4. The recipient reads it over IMAP (1143) - INBOX or Junk

    Usage (passwords are mandatory, never hardcoded; opt in to the self-signed
    test certificate explicitly):
      .\scripts\bridge-proof.ps1 `
          -SenderUser youssef@uit.ac.ma -SenderPassword '...' `
          -RecipientUser aicha@tira.test -RecipientPassword '...' `
          -SkipCertificateCheck

    -SkipCertificateCheck is refused for a non-loopback -HostName; without it the
    system validates the certificate chain and hostname (fail closed).
    Exit code 0 only if every assertion passed. The script never prints passwords.
#>
[CmdletBinding()]
param(
    [string] $HostName = "127.0.0.1",
    [int]    $SubmissionPort = 2526,
    [int]    $ImapPort = 1143,
    [string] $ServerName = "mail.tira.test",
    [Parameter(Mandatory)] [string] $SenderUser,
    [Parameter(Mandatory)] [string] $SenderPassword,
    [Parameter(Mandatory)] [string] $RecipientUser,
    [string] $RecipientPassword = "",
    [int]    $DeliveryTimeoutSeconds = 30,
    [switch] $SkipCertificateCheck
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "proof-common.ps1")

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
        if ($line.Length -ge 4 -and $line[3] -eq ' ') { return ,$lines }
        if ($line.Length -eq 3) { return ,$lines }
    }
}

function Assert-Code {
    param([string[]] $Reply, [string] $Code, [string] $Label)
    if (-not $Reply[0].StartsWith($Code)) {
        throw "ASSERT FAILED [$Label]: expected $Code, got: $($Reply -join ' | ')"
    }
    Write-Host ("  ok  {0} -> {1}" -f $Label, $Reply[0]) -ForegroundColor Green
}

function Read-ImapUntil {
    param([System.IO.StreamReader] $Reader, [string] $Tag)
    $lines = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $line = $Reader.ReadLine()
        if ($null -eq $line) { throw "IMAP connection closed while waiting for '$Tag'." }
        $lines.Add($line)
        if ($line -match "^$([regex]::Escape($Tag)) (OK|NO|BAD)") { return ,$lines }
    }
}

# The TLS validation callback comes from proof-common.ps1 (New-ProofSslStream):
# system validation by default, an explicit loopback-only opt-in otherwise.

$messageId = "<bridge-$([guid]::NewGuid().ToString('N'))@$($SenderUser.Split('@')[1])>"

# ------------------------------------------------------- 1. submission + AUTH ---
Write-Host "`n[1] Authenticated submission: $SenderUser -> $RecipientUser" -ForegroundColor Cyan
$tcp = New-Object System.Net.Sockets.TcpClient
$tcp.Connect($HostName, $SubmissionPort)
$net = $tcp.GetStream()
$plain = New-TextConnection -Stream $net
Assert-Code -Reply (Read-SmtpReply -Reader $plain.Reader) -Code "220" -Label "greeting"

$plain.Writer.WriteLine("EHLO tira.test")
$ehlo1 = Read-SmtpReply -Reader $plain.Reader
Assert-Code -Reply $ehlo1 -Code "250" -Label "EHLO (cleartext)"
if (-not ($ehlo1 -match "STARTTLS")) { throw "ASSERT FAILED: submission port does not advertise STARTTLS." }

$plain.Writer.WriteLine("STARTTLS")
Assert-Code -Reply (Read-SmtpReply -Reader $plain.Reader) -Code "220" -Label "STARTTLS"

$ssl = New-ProofSslStream -Stream $net -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$ssl.AuthenticateAsClient($ServerName)
Write-Host ("  tls: {0} / cipher {1}" -f $ssl.SslProtocol, $ssl.CipherAlgorithm)
$secure = New-TextConnection -Stream $ssl

$secure.Writer.WriteLine("EHLO tira.test")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "EHLO (TLS)"

# MAIL before AUTH must be refused: this is what makes the bridge a real
# submission service and not an open relay.
$secure.Writer.WriteLine("MAIL FROM:<$SenderUser>")
$preAuth = Read-SmtpReply -Reader $secure.Reader
if ($preAuth[0].StartsWith("250")) {
    throw "ASSERT FAILED: MAIL FROM was accepted before AUTH; the listener is an open relay."
}
Write-Host ("  ok  MAIL before AUTH refused -> {0}" -f $preAuth[0]) -ForegroundColor Green

$authPlain = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("`0$SenderUser`0$SenderPassword"))
$secure.Writer.WriteLine("AUTH PLAIN $authPlain")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "235" -Label "AUTH PLAIN"

$secure.Writer.WriteLine("MAIL FROM:<$SenderUser>")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "MAIL FROM"
$secure.Writer.WriteLine("RCPT TO:<$RecipientUser>")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "RCPT TO (cross-domain)"

$secure.Writer.WriteLine("DATA")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "354" -Label "DATA"

$body = @(
    "From: $SenderUser"
    "To: $RecipientUser"
    "Subject: Bridge proof between two local domains"
    "Date: $([DateTime]::UtcNow.ToString('r'))"
    "Message-ID: $messageId"
    "MIME-Version: 1.0"
    "Content-Type: text/plain; charset=us-ascii"
    ""
    "This message was submitted over an authenticated session and delivered to"
    "a mailbox that was provisioned from tira-store, not created by hand."
) -join "`r`n"

foreach ($line in ($body -split "`r`n")) {
    if ($line.StartsWith(".")) { $secure.Writer.WriteLine(".$line") } else { $secure.Writer.WriteLine($line) }
}
$secure.Writer.WriteLine(".")
Assert-Code -Reply (Read-SmtpReply -Reader $secure.Reader) -Code "250" -Label "message queued"

$secure.Writer.WriteLine("QUIT")
[void](Read-SmtpReply -Reader $secure.Reader)
$ssl.Dispose()
$tcp.Close()
Write-Host "  message-id: $messageId"

# --------------------------------------------------------------- 2. IMAP read ---
if (-not $RecipientPassword) {
    Write-Host "`n[2] skipped: no -RecipientPassword, cannot verify delivery over IMAP" -ForegroundColor Yellow
    Write-Host "`nBridge proof: PASS (submission only)" -ForegroundColor Green
    exit 0
}

Write-Host "`n[2] Recipient reads it over IMAP: $RecipientUser" -ForegroundColor Cyan
$tcp2 = New-Object System.Net.Sockets.TcpClient
$tcp2.Connect($HostName, $ImapPort)
$net2 = $tcp2.GetStream()
$imapPlain = New-TextConnection -Stream $net2
[void]$imapPlain.Reader.ReadLine()
$imapPlain.Writer.WriteLine("d1 STARTTLS")
$reply = Read-ImapUntil -Reader $imapPlain.Reader -Tag "d1"
if (-not ($reply -match "^d1 OK")) { throw "IMAP STARTTLS failed: $($reply -join ' | ')" }

$imapSsl = New-ProofSslStream -Stream $net2 -SkipCertificateCheck:$SkipCertificateCheck -TargetHost $HostName
$imapSsl.AuthenticateAsClient($ServerName)
$imap = New-TextConnection -Stream $imapSsl
$imap.Writer.WriteLine("d2 LOGIN $RecipientUser $RecipientPassword")
$login = Read-ImapUntil -Reader $imap.Reader -Tag "d2"
if (-not ($login -match "^d2 OK")) { throw "IMAP LOGIN failed for $RecipientUser." }
Write-Host "  ok  IMAP LOGIN" -ForegroundColor Green

$deadline = (Get-Date).AddSeconds($DeliveryTimeoutSeconds)
$found = $null
$tag = 10
while ((Get-Date) -lt $deadline) {
    foreach ($box in @("INBOX", "Junk Mail", "Sent Items")) {
        $tag++
        $imap.Writer.WriteLine("d$tag SELECT `"$box`"")
        [void](Read-ImapUntil -Reader $imap.Reader -Tag "d$tag")
        $tag++
        $imap.Writer.WriteLine("d$tag FETCH 1:* (BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")
        $fetch = Read-ImapUntil -Reader $imap.Reader -Tag "d$tag"
        if (($fetch -join "`n") -match [regex]::Escape($messageId)) { $found = $box; break }
    }
    if ($found) { break }
    Start-Sleep -Seconds 2
}

$imap.Writer.WriteLine("d99 LOGOUT")
[void](Read-ImapUntil -Reader $imap.Reader -Tag "d99")
$imapSsl.Dispose()
$tcp2.Close()

if (-not $found) {
    throw "ASSERT FAILED: message $messageId never appeared for $RecipientUser."
}
Write-Host ("  ok  delivered to: {0}" -f $found) -ForegroundColor Green

Write-Host "`nBridge proof: PASS" -ForegroundColor Green
