-- Tira Mail — identity bridge to the mail engine (P2).
--
-- PostgreSQL stays the control plane: tenants, domains, mailboxes, quotas and
-- audit. Stalwart owns message storage and the mail protocols. These columns
-- are the only link between the two, and they are deliberately explicit:
--   * `engine_id` is the id Stalwart assigned to the account. We do not choose
--     it: the engine does, so it must be stored after provisioning.
--   * `engine_state` makes the bridge resumable. A mailbox starts `pending`,
--     becomes `active` only after the engine confirmed the account, and can be
--     `failed` (with the reason) or `disabled`.
--   * No password is stored here. The provisioning secret is supplied once at
--     creation time and never persisted by the control plane.
--
-- The partial unique index allows many unprovisioned rows (engine_id NULL) but
-- forbids two mailboxes from claiming the same engine account.

ALTER TABLE mailboxes
    ADD COLUMN engine_id        TEXT,
    ADD COLUMN engine_state     TEXT NOT NULL DEFAULT 'pending',
    ADD COLUMN engine_error     TEXT,
    ADD COLUMN engine_synced_at TIMESTAMPTZ;

ALTER TABLE mailboxes
    ADD CONSTRAINT mailboxes_engine_state_valid
    CHECK (engine_state IN ('pending', 'active', 'disabled', 'failed'));

CREATE UNIQUE INDEX mailboxes_engine_id_key
    ON mailboxes (engine_id)
    WHERE engine_id IS NOT NULL;

CREATE INDEX mailboxes_engine_state_idx
    ON mailboxes (engine_state)
    WHERE engine_state <> 'active';
