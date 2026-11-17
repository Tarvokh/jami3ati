-- Tira Mail — access audit trail (P4, CNDP law 09-08).
--
-- The `audit_log` table existed since P1 but only recorded deliveries. Law
-- 09-08 on the protection of individuals with regard to the processing of
-- personal data requires that access to personal data be traceable, and that
-- the data subject be able to ask who accessed what and when. A mailbox read
-- that leaves no trace cannot answer that question.
--
-- What is recorded, and what is not
-- ---------------------------------
-- Recorded: who (token id, never the secret), what action, which object id,
-- when, from which IP, with which user agent, the outcome, and small metadata
-- (page size, offset, message size). Never recorded: message bodies, subjects,
-- or addresses beyond the object id that already exists in `messages`.
--
-- Retention: 12 months hot, then aggregated and dropped. Trimming is an
-- operational task (by hand or from cron), never a migration step: a
-- migration must not delete evidence silently.

ALTER TABLE audit_log
    ADD COLUMN request_id UUID,
    ADD COLUMN ip         INET,
    ADD COLUMN user_agent TEXT,
    ADD COLUMN result     TEXT NOT NULL DEFAULT 'ok',
    ADD COLUMN metadata   JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Fail closed on typos: `result` is read by compliance queries, so a free-text
-- value would make "how many reads were denied?" silently wrong.
ALTER TABLE audit_log
    ADD CONSTRAINT audit_log_result_valid
    CHECK (result IN ('ok', 'denied', 'error'));

CREATE INDEX audit_log_action_time_idx ON audit_log (action, at DESC);
CREATE INDEX audit_log_request_idx ON audit_log (request_id)
    WHERE request_id IS NOT NULL;

COMMENT ON TABLE audit_log IS
    'Append-only access trail. CNDP 09-08: keep 12 months, then aggregate. Never contains message bodies.';
COMMENT ON COLUMN audit_log.actor IS
    'Principal that acted: `token:<uuid>` for API reads, the envelope sender for deliveries.';
COMMENT ON COLUMN audit_log.result IS
    'ok | denied | error. `denied` is recorded for cross-tenant and out-of-scope attempts.';
