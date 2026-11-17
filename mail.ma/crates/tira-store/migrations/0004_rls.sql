-- Tira Mail — row-level security for tenant data (P4).
--
-- Why this migration exists
-- -------------------------
-- Until now every query carried `AND tenant_id = $1` by hand. That is a
-- convention, not an enforcement: one forgotten predicate in one handler
-- leaks another university's mail. This migration moves the boundary into
-- PostgreSQL itself.
--
-- Two roles, on purpose
-- ---------------------
--   * The *connection* role (`tira_owner`, `postgres` in development) runs
--     migrations, provisioning, SMTP delivery and reconciliation. It is
--     trusted to see every tenant, because it has to resolve a recipient
--     before it knows which tenant that recipient belongs to.
--   * `tira_app` is a NOLOGIN role with **no** BYPASSRLS. The HTTP read path
--     does `SET LOCAL ROLE tira_app` inside the request transaction and sets
--     `tira.tenant_id` with `set_config(..., true)`. Every table below then
--     filters itself.
--
-- The GUC is transaction-local (`is_local = true`), so a pooled connection
-- cannot carry tenant A's identity into tenant B's request.
--
-- The bootstrap path
-- ------------------
-- Resolving a bearer token and resolving an SMTP recipient both happen
-- *before* a tenant is known, so they cannot be filtered by RLS. They are
-- `SECURITY DEFINER` functions with a pinned `search_path`, executable only
-- by the owner role. They are the *only* way around the policy, and each one
-- answers a single narrow question.

-- ---------------------------------------------------------------------------
-- 1. The unprivileged role.
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'tira_app') THEN
        CREATE ROLE tira_app NOLOGIN;
    END IF;

    -- Normalise a pre-existing role: an application role must never be able
    -- to bypass the very policies it is subject to.
    ALTER ROLE tira_app NOLOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;

    IF current_user <> 'tira_app' THEN
        EXECUTE format('GRANT tira_app TO %I', current_user);
    END IF;
EXCEPTION
    WHEN insufficient_privilege THEN
        RAISE EXCEPTION
            'migration 0004 must run as a superuser (or a CREATEROLE role): '
            'it has to create tira_app and let % SET ROLE to it', current_user;
END $$;

GRANT USAGE ON SCHEMA public TO tira_app;

-- Read path needs SELECT on tenant data, and INSERT on the audit trail.
-- There is deliberately no SELECT on api_tokens and no access at all to
-- message_recipients: the token is resolved by a definer function, and the
-- recipient list is never part of an HTTP response.
GRANT SELECT ON tenants, domains, mailboxes, messages TO tira_app;
GRANT INSERT ON audit_log TO tira_app;
GRANT USAGE, SELECT ON SEQUENCE audit_log_id_seq TO tira_app;

-- ---------------------------------------------------------------------------
-- 2. The one expression every policy uses.
-- ---------------------------------------------------------------------------
-- `current_setting(..., true)` returns NULL when unset, so an unset GUC
-- matches no row: the failure mode is "see nothing", never "see everything".
-- `nullif(..., '')` guards against a caller setting the GUC to an empty
-- string instead of leaving it unset.
CREATE OR REPLACE FUNCTION tira_current_tenant() RETURNS uuid
LANGUAGE sql
STABLE
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT nullif(current_setting('tira.tenant_id', true), '')::uuid
$$;

GRANT EXECUTE ON FUNCTION tira_current_tenant() TO tira_app;

-- ---------------------------------------------------------------------------
-- 3. Policies.
-- ---------------------------------------------------------------------------
-- `tenants` is filtered by primary key; every other table by `tenant_id`.
-- No table uses FORCE ROW LEVEL SECURITY: the owner role must still be able
-- to provision and deliver across tenants. That is safe *only* because the
-- application role is separate and has no BYPASSRLS.
ALTER TABLE tenants  ENABLE ROW LEVEL SECURITY;
ALTER TABLE domains  ENABLE ROW LEVEL SECURITY;
ALTER TABLE mailboxes ENABLE ROW LEVEL SECURITY;
ALTER TABLE messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenants_isolation ON tenants
    FOR ALL TO tira_app
    USING (id = tira_current_tenant())
    WITH CHECK (id = tira_current_tenant());

CREATE POLICY domains_isolation ON domains
    FOR ALL TO tira_app
    USING (tenant_id = tira_current_tenant())
    WITH CHECK (tenant_id = tira_current_tenant());

CREATE POLICY mailboxes_isolation ON mailboxes
    FOR ALL TO tira_app
    USING (tenant_id = tira_current_tenant())
    WITH CHECK (tenant_id = tira_current_tenant());

CREATE POLICY messages_isolation ON messages
    FOR ALL TO tira_app
    USING (tenant_id = tira_current_tenant())
    WITH CHECK (tenant_id = tira_current_tenant());

-- Audit rows are write-only for the application role: it may append an entry
-- for its own tenant and may not read, update, or delete anyone's trail.
CREATE POLICY audit_log_isolation ON audit_log
    FOR ALL TO tira_app
    USING (tenant_id = tira_current_tenant())
    WITH CHECK (tenant_id = tira_current_tenant());

-- `api_tokens` and `message_recipients` have RLS enabled but **no policy and
-- no grant**: deny-all for the application role. They are reached only through
-- the owner role (token issuance) or a definer function (token resolution).
ALTER TABLE api_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE message_recipients ENABLE ROW LEVEL SECURITY;

-- ---------------------------------------------------------------------------
-- 4. The two pre-tenant lookups.
-- ---------------------------------------------------------------------------
-- Resolves a SHA-256 token hash to its scope and records use. The hash is the
-- input, so the caller already possesses the secret; the function never
-- returns the hash back.
CREATE OR REPLACE FUNCTION tira_resolve_token(p_hash bytea)
RETURNS TABLE (
    token_id         uuid,
    token_tenant_id  uuid,
    token_mailbox_id uuid,
    token_label      text,
    token_created_at timestamptz,
    token_expires_at timestamptz,
    token_used_at    timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_id uuid;
BEGIN
    SELECT t.id INTO v_id
      FROM api_tokens AS t
     WHERE t.token_hash = p_hash
       AND t.revoked_at IS NULL
       AND (t.expires_at IS NULL OR t.expires_at > now());

    IF v_id IS NULL THEN
        RETURN;
    END IF;

    UPDATE api_tokens AS t SET last_used_at = now() WHERE t.id = v_id;

    RETURN QUERY
        SELECT t.id, t.tenant_id, t.mailbox_id, t.label,
               t.created_at, t.expires_at, t.last_used_at
          FROM api_tokens AS t
         WHERE t.id = v_id;
END;
$$;

-- Resolves `local@domain` to the owning mailbox. Called by SMTP at RCPT time,
-- before any tenant context exists.
CREATE OR REPLACE FUNCTION tira_resolve_recipient(p_local text, p_domain text)
RETURNS TABLE (
    mailbox_id        uuid,
    mailbox_tenant_id uuid,
    mailbox_domain_id uuid,
    address           text,
    quota_bytes       bigint,
    used_bytes        bigint
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT m.id, m.tenant_id, m.domain_id,
           m.local_part || '@' || d.name,
           m.quota_bytes, m.used_bytes
      FROM mailboxes AS m
      JOIN domains  AS d ON d.id = m.domain_id
     WHERE m.local_part = p_local
       AND d.name = p_domain
$$;

-- Only the owner role may call the two escape hatches. PUBLIC gets nothing.
REVOKE ALL ON FUNCTION tira_resolve_token(bytea) FROM PUBLIC;
REVOKE ALL ON FUNCTION tira_resolve_recipient(text, text) FROM PUBLIC;

DO $$
BEGIN
    EXECUTE format(
        'GRANT EXECUTE ON FUNCTION tira_resolve_token(bytea) TO %I', current_user);
    EXECUTE format(
        'GRANT EXECUTE ON FUNCTION tira_resolve_recipient(text, text) TO %I', current_user);
END $$;

COMMENT ON ROLE tira_app IS
    'Read-path role for Tira Mail. NOLOGIN, no BYPASSRLS: tenant isolation is enforced by RLS, not by SQL predicates.';
COMMENT ON FUNCTION tira_resolve_token(bytea) IS
    'Pre-tenant token resolution (SECURITY DEFINER). The only way to read api_tokens while RLS is on.';
COMMENT ON FUNCTION tira_resolve_recipient(text, text) IS
    'Pre-tenant recipient resolution for SMTP (SECURITY DEFINER).';
