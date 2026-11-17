-- Tira Mail - machine-callable outbound submission (Jami3ati integration).
--
-- The browser compose path is form-first: it tries the relay, and only parks a
-- *failed* attempt in `outbound_queue`. A machine caller (Jami3ati) cannot
-- accept "delivered or nothing": it needs a durable, idempotent *accept* that
-- the background worker then drains. This migration extends the existing queue
-- instead of creating a second one, so there stays exactly one retry policy,
-- one dead-letter ledger and one audit trail.
--
-- Why explicit columns instead of a `headers JSONB`
-- -------------------------------------------------
-- Arbitrary header input is a mail-security bug (CRLF injection, a forged
-- `Bcc:` or `From:`). The four fields below are the only headers the
-- integration needs, and each is validated in Rust before it reaches the
-- queue. The set of possible headers is therefore closed at compile time.
--
-- `idempotency_key` makes acceptance at-most-once per tenant: a retried POST
-- (the network dropped the first 202) returns the same queue row instead of
-- composing a second message. Browser-compose rows keep it NULL.

ALTER TABLE outbound_queue
    ADD COLUMN idempotency_key  TEXT,
    ADD COLUMN correlation_id   TEXT,
    ADD COLUMN list_unsubscribe TEXT,
    ADD COLUMN reply_to         TEXT;

CREATE UNIQUE INDEX outbound_queue_idempotency_key_idx
    ON outbound_queue (tenant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

COMMENT ON COLUMN outbound_queue.idempotency_key IS
    'Caller-supplied dedupe key, unique per tenant. NULL for browser-compose rows.';

COMMENT ON COLUMN outbound_queue.correlation_id IS
    'X-Jami3ati-Notification value: ties the delivered mail to the notification row in the calling system.';

COMMENT ON COLUMN outbound_queue.list_unsubscribe IS
    'Ready-to-send List-Unsubscribe value, e.g. <https://jami3ati.ma/u/{token}>. Validated before insert.';

COMMENT ON COLUMN outbound_queue.reply_to IS
    'Optional Reply-To address. Validated before insert.';
