-- Tira Mail — initial schema (P1).
--
-- Design notes that matter:
--   * `tenant_id` is on every table that holds user data, so a future
--     `ROW LEVEL SECURITY` policy can be added without a data migration.
--   * Addresses and domain names are stored lowercase and enforced by CHECK,
--     because `Aicha@UCA.MA` and `aicha@uca.ma` are the same mailbox.
--   * Blobs live outside PostgreSQL (see `blob.rs`); the row stores only the
--     content-addressed key and the size, never the bytes.
--   * No `ON DELETE CASCADE` from `tenants` to `audit_log`: deleting a tenant
--     must not erase the record that it existed.

CREATE TABLE tenants (
    id         UUID PRIMARY KEY,
    name       TEXT NOT NULL,
    cndp_ref   TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE domains (
    id               UUID PRIMARY KEY,
    tenant_id        UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    dkim_private_enc BYTEA,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT domains_name_lowercase CHECK (name = lower(name))
);

CREATE UNIQUE INDEX domains_name_key ON domains (name);
CREATE INDEX domains_tenant_idx ON domains (tenant_id);

CREATE TABLE mailboxes (
    id           UUID PRIMARY KEY,
    tenant_id    UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    domain_id    UUID NOT NULL REFERENCES domains (id) ON DELETE CASCADE,
    local_part   TEXT NOT NULL,
    display_name TEXT,
    quota_bytes  BIGINT NOT NULL DEFAULT 5368709120, -- 5 GiB
    used_bytes   BIGINT NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT mailboxes_local_part_lowercase CHECK (local_part = lower(local_part)),
    CONSTRAINT mailboxes_quota_positive CHECK (quota_bytes > 0),
    CONSTRAINT mailboxes_used_non_negative CHECK (used_bytes >= 0),
    CONSTRAINT mailboxes_used_within_quota CHECK (used_bytes <= quota_bytes)
);

CREATE UNIQUE INDEX mailboxes_address_key ON mailboxes (domain_id, local_part);
CREATE INDEX mailboxes_tenant_idx ON mailboxes (tenant_id);

CREATE TABLE messages (
    id             UUID PRIMARY KEY,
    tenant_id      UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    mailbox_id     UUID NOT NULL REFERENCES mailboxes (id) ON DELETE CASCADE,
    message_id_hdr TEXT,
    subject        TEXT,
    from_addr      TEXT NOT NULL,
    size_bytes     BIGINT NOT NULL,
    blob_key       TEXT NOT NULL,
    sha256         TEXT NOT NULL,
    received_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT messages_size_positive CHECK (size_bytes > 0)
);

CREATE INDEX messages_mailbox_time_idx ON messages (mailbox_id, received_at DESC);
CREATE INDEX messages_sha_idx ON messages (sha256);
CREATE INDEX messages_msgid_idx ON messages (message_id_hdr);
CREATE INDEX messages_tenant_idx ON messages (tenant_id);

CREATE TABLE message_recipients (
    message_id UUID NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    address    TEXT NOT NULL,
    PRIMARY KEY (message_id, address)
);

CREATE TABLE audit_log (
    id        BIGSERIAL PRIMARY KEY,
    tenant_id UUID REFERENCES tenants (id) ON DELETE SET NULL,
    actor     TEXT NOT NULL,
    action    TEXT NOT NULL,
    object    TEXT,
    at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX audit_log_tenant_time_idx ON audit_log (tenant_id, at DESC);
