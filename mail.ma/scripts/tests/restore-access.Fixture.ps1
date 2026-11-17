# Explicit synthetic fixture, persisted through real PostgreSQL and real tar.
function New-MailFixture {
    param($Cluster)
    $repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
    Sql $Cluster 'postgres' 'CREATE DATABASE b2_source;' | Out-Null
    Sql $Cluster 'b2_source' 'CREATE TABLE schema_version(version text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now());' | Out-Null
    foreach ($file in Get-ChildItem -LiteralPath (Join-Path $repo 'crates\tira-store\migrations') -Filter '*.sql' | Sort-Object Name) {
        $text=[IO.File]::ReadAllText($file.FullName)
        Sql $Cluster 'b2_source' ("BEGIN;`n"+$text+"`nINSERT INTO schema_version(version) VALUES ('$($file.BaseName)');`nCOMMIT;") | Out-Null
    }
    $blobRoot=Join-Path $script:Lab 'blobs'
    New-Item -ItemType Directory -Path $blobRoot | Out-Null
    $script:Fixtures=@()
    foreach ($n in 1,2) {
        $t="00000000-0000-0000-0000-00000000000$n"
        $d="10000000-0000-0000-0000-00000000000$n"
        $m="20000000-0000-0000-0000-00000000000$n"
        $id="30000000-0000-0000-0000-00000000000$n"
        $token="tira_batch2_synthetic_$n"
        $sha=[Security.Cryptography.SHA256]::Create()
        try { $tokenHash=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($token)))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
        $mime="From: fixture@sender.invalid`r`nTo: test@tenant$n.invalid`r`nSubject: SYNTHETIC RESTORE FIXTURE $n`r`nMIME-Version: 1.0`r`nContent-Type: multipart/mixed; boundary=b2boundary`r`n`r`n--b2boundary`r`nContent-Type: text/plain; charset=utf-8`r`n`r`nNot user mail. Restore test tenant $n.`r`n--b2boundary`r`nContent-Type: application/octet-stream`r`nContent-Disposition: attachment; filename=fixture.bin`r`nContent-Transfer-Encoding: base64`r`n`r`nAAECAwQFBgcICQ==`r`n--b2boundary--`r`n"
        $bytes=[Text.Encoding]::UTF8.GetBytes($mime)
        $sha=[Security.Cryptography.SHA256]::Create()
        try { $hash=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
        $key=$hash.Substring(0,2)+'/'+$hash.Substring(2,2)+'/'+$hash
        $file=Join-Path $blobRoot $key
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($file)) -Force | Out-Null
        [IO.File]::WriteAllBytes($file,$bytes)
        Sql $Cluster 'b2_source' @"
BEGIN;
INSERT INTO tenants(id,name) VALUES ('$t','SYNTHETIC TEST TENANT $n');
INSERT INTO domains(id,tenant_id,name,dkim_private_enc) VALUES ('$d','$t','tenant$n.invalid',decode('deadbeef','hex'));
INSERT INTO mailboxes(id,tenant_id,domain_id,local_part,used_bytes) VALUES ('$m','$t','$d','test',$($bytes.Length));
INSERT INTO messages(id,tenant_id,mailbox_id,subject,from_addr,size_bytes,blob_key,sha256) VALUES ('$id','$t','$m','SYNTHETIC RESTORE FIXTURE $n','fixture@sender.invalid',$($bytes.Length),'$key','$hash');
INSERT INTO message_recipients(message_id,address) VALUES ('$id','test@tenant$n.invalid');
INSERT INTO api_tokens(id,tenant_id,mailbox_id,label,token_hash) VALUES ('$id','$t','$m','SYNTHETIC FIXTURE',decode('$tokenHash','hex'));
INSERT INTO outbound_queue(tenant_id,mailbox_id,from_addr,recipients,subject,body,status) VALUES ('$t','$m','test@tenant$n.invalid','none@invalid.invalid','SYNTHETIC','DO NOT SEND','failed');
INSERT INTO bounces(tenant_id,mailbox_id,recipient,reason) VALUES ('$t','$m','none@invalid.invalid','SYNTHETIC TEST');
INSERT INTO audit_log(tenant_id,actor,action) VALUES ('$t','SYNTHETIC FIXTURE','fixture.seed');
COMMIT;
"@ | Out-Null
        $script:Fixtures += [pscustomobject]@{Tenant=$t;Mailbox=$m;Message=$id;Token=$token;Hash=$hash;Bytes=$bytes;Key=$key}
    }
    return $blobRoot
}
function Run-BackupFixture {
    param($Cluster,[string]$BlobRoot,[string]$Scripts=(Join-Path $PSScriptRoot '..'))
    $out=Join-Path $script:Lab ('backups-'+[guid]::NewGuid().ToString('N').Substring(0,8))
    New-Item -ItemType Directory -Path $out | Out-Null
    & (Join-Path $Scripts 'backup.ps1') -DatabaseUrl "postgres://b2_admin@127.0.0.1:$($Cluster.Port)/b2_source" -BlobRoot $BlobRoot -OutDir $out -PgBin $script:PgBin
    $dirs=@(Get-ChildItem -LiteralPath $out -Directory)
    if ($dirs.Count -ne 1) { throw 'Expected exactly one generated backup' }
    return $dirs[0].FullName
}
