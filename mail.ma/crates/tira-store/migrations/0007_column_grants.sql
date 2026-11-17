-- Tira Mail — column-level least privilege + cross-tenant message integrity.
--
-- Two independent hardenings, both defence in depth around the RLS boundary
-- that migration 0004 established.
--
-- M-08: the DKIM private key must not be readable by the web role
-- -------------------------------------------------------------
-- 0004 granted `SELECT ON domains` table-wide, which includes
-- `dkim_private_enc` — the encrypted DKIM signing key. No HTTP read path
-- selects that column today (they all join `domains` only for `d.name`), but
-- a table-wide grant means one future `SELECT *`, one compromised handler, or
-- one SQL injection away from exfiltrating a tenant's signing key.
--
-- The fix is column-level least privilege: revoke the table-wide grant and
-- grant SELECT on exactly the columns the read path uses. `dkim_private_enc`
-- stays reachable only by the owner role (engine provisioning), which is the
-- same trust boundary that already protects `api_tokens` and
-- `mailbox_credentials`. RLS still filters the rows; this filters the columns.
--
-- M-11: a message can never claim the wrong tenant
-- ----------------------------------------------
-- `messages.mailbox_id` references `mailboxes(id)` and `messages.tenant_id`
-- references `tenants(id)` independently. Nothing in the schema stops a row
-- whose `mailbox_id` belongs to tenant A while its `tenant_id` says tenant B.
-- The application always sets them consistently, but a bug or a manual write
-- could desynchronise them, and the RLS policy on `messages` filters by
-- `tenant_id` — so a desynchronised row would be visible to the wrong tenant.
--
-- A composite foreign key makes the database enforce the invariant: the pair
-- (mailbox_id, tenant_id) must exist as a pair in `mailboxes`. The UNIQUE
-- constraint on mailboxes(id, tenant_id) is the required referential target
-- (id alone is already the primary key, so this adds no new uniqueness).

-- ---------------------------------------------------------------------------
-- 1. Column-level SELECT on domains for the application role.
-- ---------------------------------------------------------------------------
REVOKE SELECT ON domains FROM tira_app;
GRANT SELECT (id, tenant_id, name, created_at) ON domains TO tira_app;

COMMENT ON COLUMN domains.dkim_private_enc IS
    'Encrypted DKIM signing key. Owner-role only: deliberately excluded from the tira_app column grant (migration 0007).';

-- ---------------------------------------------------------------------------
-- 2. Composite integrity: a message's tenant must match its mailbox's tenant.
-- ---------------------------------------------------------------------------
ALTER TABLE mailboxes
    ADD CONSTRAINT mailboxes_id_tenant_key UNIQUE (id, tenant_id);

ALTER TABLE messages
    ADD CONSTRAINT messages_mailbox_tenant_fk
    FOREIGN KEY (mailbox_id, tenant_id)
    REFERENCES mailboxes (id, tenant_id)
    ON DELETE CASCADE;

COMMENT ON CONSTRAINT messages_mailbox_tenant_fk ON messages IS
    'Enforces messages.tenant_id = owning mailbox tenant. Prevents a desynchronised row from being exposed to the wrong tenant by the messages RLS policy.';