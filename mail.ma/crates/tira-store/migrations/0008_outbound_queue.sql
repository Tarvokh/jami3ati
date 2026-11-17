-- Tira Mail — durable outbound queue (Phase 2, module: outbound queue).
--
-- Before this migration an outbound submission was all-or-nothing: either the
-- relay accepted the message (and the delivery handler persisted it) or the
-- attempt was lost with the TCP reset. The human's words were therefore never
-- retried, and a transient relay blip during the demo meant a lost send.
--
-- The control plane now records every composed send here so the background
-- retry worker can replay it with exponential backoff instead of dropping it.
--
-- What is stored
-- -------------
-- The composed *intent*, not the raw bytes: envelope sender, comma-separated
-- recipient list, subject and body. The RFC 5322 message is rebuilt from these
-- fields on every attempt, which keeps retries free of stale copies and keeps
-- the queue free of blobs that would need their own retention rules.
--
-- Lifecycle
-- ----------
--   pending -> sent    delivery attempt succeeded (the delivery handler persists
--                      the message exactly as before);
--   pending -> pending failed attempt, rescheduled with backoff (attempts += 1);
--   pending -> failed  attempts exhausted, kept as a dead letter (audit trail).
--
-- `attempts` counts dispatches; `max_attempts` is the operator ceiling (3 in
-- the demo policy). Backoff is exponential from a 30s base, capped at 1h.

CREATE TABLE outbound_queue (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    mailbox_id       UUID NOT NULL,
    from_addr        TEXT NOT NULL,
    recipients       TEXT NOT NULL,
    subject          TEXT NOT NULL,
    body             TEXT NOT NULL,
    attempts         INTEGER NOT NULL DEFAULT 0,
    max_attempts     INTEGER NOT NULL DEFAULT 3,
    status           TEXT NOT NULL DEFAULT 'pending'
        CONSTRAINT outbound_queue_status_valid
        CHECK (status IN ('pending', 'sent', 'failed')),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error       TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT outbound_queue_tenant_mailbox_fk
        FOREIGN KEY (mailbox_id, tenant_id)
        REFERENCES mailboxes (id, tenant_id) ON DELETE CASCADE
);

CREATE INDEX outbound_queue_dispatch_idx
    ON outbound_queue (next_attempt_at)
    WHERE status = 'pending';

-- Same isolation shape as every other tenant table, so the unprivileged path
-- can never reach another tenant's queue rows. The retry worker itself runs as
-- the owner role (superuser, bypasses RLS) through the same pool.
ALTER TABLE outbound_queue ENABLE ROW LEVEL SECURITY;

CREATE POLICY outbound_queue_isolation ON outbound_queue
    FOR ALL TO tira_app
    USING (tenant_id = tira_current_tenant())
    WITH CHECK (tenant_id = tira_current_tenant());

GRANT SELECT, INSERT, UPDATE ON outbound_queue TO tira_app;

COMMENT ON TABLE outbound_queue IS
    'Durable outbound send queue (Phase 2). Composed send intents with exponential-backoff retries; `failed` rows are dead letters retained for the audit trail.';
