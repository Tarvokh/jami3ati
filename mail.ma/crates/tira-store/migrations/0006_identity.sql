-- Tira Mail — local control-plane credentials (P5 identity).
--
-- Why a separate table and not a column on `mailboxes`:
--   * `mailboxes` is readable by `tira_app`, the RLS read role. A password
--     hash in a readable column would be one forgotten `SELECT *` away from
--     the HTTP read path.
--   * `mailbox_credentials` is deny-all to `tira_app`: RLS is enabled with no
--     policy and no GRANT. Only the owner connection (the login handler) can
--     read or write it, exactly like `api_tokens`.
--
-- What this is *not*: a Stalwart/IMAP password. Stalwart remains the mail
-- engine; this credential authenticates the Tira web session. The UI says so
-- on the login page and in the profile, so nobody mistakes one for the other.
--
-- The hash is a PHC string (`$argon2id$v=19$...`) produced by `tira-auth`.
-- The CHECK rejects plaintext or a fast hash before it can reach a row.

CREATE TABLE mailbox_credentials (
    mailbox_id    UUID PRIMARY KEY REFERENCES mailboxes(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT mailbox_credentials_argon2id
        CHECK (password_hash LIKE '$argon2id$%')
);

ALTER TABLE mailbox_credentials ENABLE ROW LEVEL SECURITY;
-- Deliberately no policy and no GRANT: `tira_app` sees nothing. The owner
-- role is the only reader, and only through the narrow methods in
-- `tira-store/src/identity.rs`.

COMMENT ON TABLE mailbox_credentials IS
    'Tira control-plane credentials (Argon2id PHC). Deny-all to tira_app; owner-only. Not Stalwart/IMAP passwords.';
