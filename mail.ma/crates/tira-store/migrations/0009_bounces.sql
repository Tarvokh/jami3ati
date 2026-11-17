-- Tira Mail — bounce (DSN) ledger (Phase 2, module: bounce).
--
-- A durable bounce record is written when an outbound send exhausts its retry
-- budget and is dead-lettered. The delivery itself failed permanently (for
-- example a `550 5.1.1 No such recipient here` from the receiving MTA), so the
-- human's message did not land and the intent is closed as bounced.
--
-- The ledger keeps the *reason* of the permanent failure (the last SMTP error)
-- next to the recipient, so an operator — or the compliance trail — can answer
-- "why did that send never arrive?" without replaying the queue.
--
-- Like every tenant table it is row-level isolated for `tira_app`; the worker
-- writes through the owner role (superuser, bypasses RLS) exactly like the rest
-- of the delivery path.

CREATE TABLE bounces (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    mailbox_id  UUID NOT NULL,
    recipient   TEXT NOT NULL,
    reason      TEXT NOT NULL,
    permanent   BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT bounces_mailbox_tenant_fk
        FOREIGN KEY (mailbox_id, tenant_id)
        REFERENCES mailboxes (id, tenant_id) ON DELETE CASCADE
);

CREATE INDEX bounces_tenant_time_idx ON bounces (tenant_id, created_at DESC);

ALTER TABLE bounces ENABLE ROW LEVEL SECURITY;

CREATE POLICY bounces_isolation ON bounces
    FOR ALL TO tira_app
    USING (tenant_id = tira_current_tenant())
    WITH CHECK (tenant_id = tira_current_tenant());

GRANT SELECT, INSERT, UPDATE ON bounces TO tira_app;

COMMENT ON TABLE bounces IS
    'Durable bounce ledger (Phase 2). Permanent outbound delivery failures closed as bounces, with the final SMTP reason.';
