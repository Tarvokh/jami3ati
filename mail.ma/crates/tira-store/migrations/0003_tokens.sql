-- Tira Mail — bearer tokens for the read API (P4).
--
-- Why a table and not a static secret in the environment:
--   * one token per tenant, not per deployment;
--   * revoking one customer must not lock out another;
--   * a leaked token can be traced to a tenant and a creation time.
--
-- Only the SHA-256 of the secret is stored. The secret itself is printed once
-- by `tira-token` and never persisted by the control plane. Tokens are
-- high-entropy random values, so a fast hash is correct here; Argon2 is for
-- human passwords, and would make every request wait ~50ms.
--
-- `mailbox_id` narrows a token to a single mailbox. NULL means "every mailbox
-- in the tenant" (still scoped: never cross-tenant).

CREATE TABLE api_tokens (
    id           UUID PRIMARY KEY,
    tenant_id    UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    mailbox_id   UUID REFERENCES mailboxes(id) ON DELETE CASCADE,
    label        TEXT NOT NULL,
    token_hash   BYTEA NOT NULL UNIQUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    CONSTRAINT api_tokens_label_not_empty CHECK (length(label) > 0)
);

CREATE INDEX api_tokens_tenant_idx ON api_tokens (tenant_id);

-- Fast path of the middleware: valid, unexpired, unrevoked tokens.
CREATE INDEX api_tokens_live_idx
    ON api_tokens (token_hash)
    WHERE revoked_at IS NULL;
