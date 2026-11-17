#requires -Version 5.1
# Local, versioned restore policy. This is NOT a roles dump and never replays
# cluster users/passwords or changes an existing role. Load backup-common first.
Set-StrictMode -Version Latest

function Assert-RestoreSchema {
    param([Parameter(Mandatory)] [string[]] $Versions)
    $supported = @('0001_init','0002_engine','0003_tokens','0004_rls','0005_audit',
        '0006_identity','0007_column_grants','0008_outbound_queue','0009_bounces')
    if (($Versions -join ',') -cne ($supported -join ',')) {
        throw 'Unsupported restore access schema. This policy supports exactly migrations 0001_init through 0009_bounces; update and test the access contract before restoring another schema.'
    }
}

function Initialize-RestoreRole {
    param([string] $Psql, [string] $DbHost, [int] $Port, [string] $DbUser, [switch] $CreateAppRole)
    # Role creation is cluster-wide: opt-in only, transactional with membership.
    # Existing attributes/memberships are checked, never normalised or revoked.
    $create = if ($CreateAppRole) { @'
        CREATE ROLE tira_app NOLOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION;
        EXECUTE format('GRANT tira_app TO %I', current_user);
'@ } else { "        RAISE EXCEPTION 'tira_app is missing; provision it explicitly or use -CreateAppRole on an isolated restore server';" }
    $sql = @'
-- tira-restore-role/1
BEGIN;
DO $role$
DECLARE r record;
BEGIN
    IF current_user = 'tira_app' THEN RAISE EXCEPTION 'restore must use a separate owner role'; END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'tira_app') THEN
__CREATE__
    END IF;
    SELECT * INTO STRICT r FROM pg_catalog.pg_roles WHERE rolname = 'tira_app';
    IF r.rolcanlogin OR r.rolsuper OR r.rolbypassrls OR r.rolcreatedb OR r.rolcreaterole OR r.rolreplication
       OR EXISTS (SELECT 1 FROM pg_catalog.pg_auth_members WHERE member = r.oid) THEN
        RAISE EXCEPTION 'unsafe existing tira_app role; refusing to change it or use it for restore';
    END IF;
END
$role$;
SET LOCAL ROLE tira_app;
RESET ROLE;
COMMIT;
'@
    $sql = $sql.Replace('__CREATE__', $create)
    $result = Invoke-Native -Exe $Psql -Arguments @('-h',$DbHost,'-p',"$Port",'-U',$DbUser,'-w',
        '--dbname=postgres','--no-psqlrc','--set=ON_ERROR_STOP=1',"--command=$sql")
    if ($result.ExitCode -ne 0) { throw "restore role preflight failed: $($result.Output)" }
}

function Protect-RestoreDatabase {
    param([string] $Psql, [string] $DatabaseUrl, [string] $ScratchDb)
    if ($ScratchDb -cnotmatch '\Atira_restore_[a-z0-9_]{1,50}\z') { throw 'Unsafe scratch database name' }
    # Executed on the acquired, empty template0 database BEFORE pg_restore.
    # No public schema access; newly restored functions never get default PUBLIC
    # EXECUTE. This closes the interval before the explicit access policy runs.
    $sql = @'
-- tira-restore-quarantine/1
BEGIN;
REVOKE ALL ON DATABASE "__DB__" FROM PUBLIC;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
ALTER DEFAULT PRIVILEGES REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;
COMMIT;
'@
    $sql = $sql.Replace('__DB__', $ScratchDb)
    $result = Invoke-Native -Exe $Psql -Arguments @("--dbname=$DatabaseUrl",'--no-psqlrc','--no-password',
        '--set=ON_ERROR_STOP=1',"--command=$sql")
    if ($result.ExitCode -ne 0) { throw "scratch access quarantine failed: $($result.Output)" }
}

function Restore-TiraAccess {
    param([string] $Psql, [string] $DatabaseUrl)
    # Reconstruct only the shipped application's grants, never arbitrary donor
    # ACLs. Keep --no-owner/--no-privileges: the invoking connection owns objects.
    # All local access changes commit together, or none do. No data is changed.
    $sql = @'
-- tira-restore-access/1
BEGIN;
SET LOCAL search_path = pg_catalog;
DO $check$
DECLARE actual text[];
BEGIN
    SELECT array_agg(version ORDER BY version) INTO actual FROM public.schema_version;
    IF actual IS DISTINCT FROM ARRAY['0001_init','0002_engine','0003_tokens','0004_rls','0005_audit','0006_identity','0007_column_grants','0008_outbound_queue','0009_bounces']::text[] THEN
        RAISE EXCEPTION 'restored schema has no supported access contract';
    END IF;
    SELECT array_agg(c.relname::text ORDER BY c.relname) INTO actual
      FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
      WHERE n.nspname='public' AND c.relkind IN ('r','p');
    IF actual IS DISTINCT FROM ARRAY['api_tokens','audit_log','bounces','domains','mailbox_credentials','mailboxes','message_recipients','messages','outbound_queue','schema_version','tenants']::text[] THEN
        RAISE EXCEPTION 'unexpected restored tables; refusing a partial access policy';
    END IF;
    IF EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
               WHERE n.nspname='public' AND c.relkind IN ('r','p','S') AND c.relowner <> current_user::regrole) THEN
        RAISE EXCEPTION 'restored objects must belong to the restore connection, never tira_app';
    END IF;
    IF (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public') <> 3
       OR NOT EXISTS (SELECT 1 FROM pg_proc WHERE oid='public.tira_resolve_token(bytea)'::regprocedure AND prosecdef AND proowner=current_user::regrole AND proconfig @> ARRAY['search_path=public, pg_temp'])
       OR NOT EXISTS (SELECT 1 FROM pg_proc WHERE oid='public.tira_resolve_recipient(text,text)'::regprocedure AND prosecdef AND proowner=current_user::regrole AND proconfig @> ARRAY['search_path=public, pg_temp']) THEN
        RAISE EXCEPTION 'unexpected restored function ownership or configuration';
    END IF;
END
$check$;

REVOKE ALL ON SCHEMA public FROM PUBLIC, tira_app;
GRANT USAGE ON SCHEMA public TO tira_app;
REVOKE ALL ON ALL TABLES IN SCHEMA public FROM PUBLIC, tira_app;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA public FROM PUBLIC, tira_app;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA public FROM PUBLIC, tira_app;
-- Table REVOKE alone does not remove column-level grants.
DO $columns$
DECLARE t record;
BEGIN
    FOR t IN SELECT c.relname, string_agg(quote_ident(a.attname), ',' ORDER BY a.attnum) AS cols
        FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
        JOIN pg_attribute a ON a.attrelid=c.oid AND a.attnum>0 AND NOT a.attisdropped
        WHERE n.nspname='public' AND c.relkind IN ('r','p') GROUP BY c.relname
    LOOP
        EXECUTE format('REVOKE ALL (%s) ON public.%I FROM PUBLIC, tira_app', t.cols, t.relname);
    END LOOP;
END
$columns$;
GRANT SELECT ON public.tenants, public.mailboxes, public.messages TO tira_app;
GRANT SELECT (id,tenant_id,name,created_at) ON public.domains TO tira_app;
GRANT INSERT ON public.audit_log TO tira_app;
GRANT USAGE, SELECT ON SEQUENCE public.audit_log_id_seq TO tira_app;
GRANT SELECT, INSERT, UPDATE ON public.outbound_queue, public.bounces TO tira_app;
GRANT EXECUTE ON FUNCTION public.tira_current_tenant() TO tira_app;
-- The two SECURITY DEFINER escape hatches stay owner-only.

-- Reinstate the policy shape rather than trusting a grant-only check. This
-- affects only this newly acquired scratch DB, not the original cluster/store.
DO $policies$
DECLARE p record; t text; field text;
BEGIN
    FOR p IN SELECT c.relname, pol.polname FROM pg_policy pol JOIN pg_class c ON c.oid=pol.polrelid
        JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'
    LOOP EXECUTE format('DROP POLICY %I ON public.%I',p.polname,p.relname); END LOOP;
    FOREACH t IN ARRAY ARRAY['tenants','domains','mailboxes','messages','audit_log','api_tokens','message_recipients','mailbox_credentials','outbound_queue','bounces']
    LOOP
        EXECUTE format('ALTER TABLE public.%I ENABLE ROW LEVEL SECURITY',t);
        EXECUTE format('ALTER TABLE public.%I NO FORCE ROW LEVEL SECURITY',t);
    END LOOP;
    FOREACH t IN ARRAY ARRAY['tenants','domains','mailboxes','messages','audit_log','outbound_queue','bounces']
    LOOP
        field := CASE WHEN t='tenants' THEN 'id' ELSE 'tenant_id' END;
        EXECUTE format('CREATE POLICY %I ON public.%I TO tira_app USING (%I = public.tira_current_tenant()) WITH CHECK (%I = public.tira_current_tenant())',t||'_isolation',t,field,field);
    END LOOP;
END
$policies$;

DO $verify$
BEGIN
    IF NOT has_table_privilege('tira_app','public.messages','SELECT')
       OR NOT has_column_privilege('tira_app','public.domains','name','SELECT')
       OR has_column_privilege('tira_app','public.domains','dkim_private_enc','SELECT')
       OR has_table_privilege('tira_app','public.api_tokens','SELECT')
       OR has_table_privilege('tira_app','public.mailbox_credentials','SELECT')
       OR has_table_privilege('tira_app','public.message_recipients','SELECT')
       OR has_table_privilege('tira_app','public.audit_log','SELECT')
       OR has_function_privilege('tira_app','public.tira_resolve_token(bytea)','EXECUTE')
       OR has_function_privilege('tira_app','public.tira_resolve_recipient(text,text)','EXECUTE') THEN
        RAISE EXCEPTION 'restore least-privilege check failed';
    END IF;
END
$verify$;
-- Exercise actual SQL under the low-privilege role. Unset identity sees zero
-- tenant rows, including when the archive contains other tenants' messages.
SET LOCAL ROLE tira_app;
SELECT set_config('tira.tenant_id','',true);
DO $rls$
BEGIN
    IF EXISTS (SELECT 1 FROM public.tenants) OR EXISTS (SELECT 1 FROM public.mailboxes)
       OR EXISTS (SELECT 1 FROM public.domains) OR EXISTS (SELECT 1 FROM public.messages)
       OR EXISTS (SELECT 1 FROM public.outbound_queue) OR EXISTS (SELECT 1 FROM public.bounces) THEN
        RAISE EXCEPTION 'restored RLS exposes rows without a tenant context';
    END IF;
END
$rls$;
RESET ROLE;
COMMIT;
'@
    $result = Invoke-Native -Exe $Psql -Arguments @("--dbname=$DatabaseUrl",'--no-psqlrc','--no-password',
        '--set=ON_ERROR_STOP=1',"--command=$sql")
    if ($result.ExitCode -ne 0) { throw "restore access contract failed: $($result.Output)" }
}
