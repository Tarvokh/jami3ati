//! Identity: local Tira credentials, profiles, and web sessions.
//!
//! This is deliberately **not** Stalwart authentication. Stalwart remains the
//! mail engine; these rows authenticate a person to the Tira web UI. The login
//! page and the profile both say that out loud, so the distinction survives
//! into the product instead of living only in this comment.
//!
//! Security shape:
//!
//! - `mailbox_credentials` is deny-all to `tira_app` (see migration 0006).
//!   Every method here uses the owner connection, like token issuance.
//! - Password hashing/verification happens in `tira-api` through `tira-auth`
//!   (Argon2id). The store only persists and returns the PHC string.
//! - Session tokens reuse `api_tokens`; they are mailbox-scoped, expiring, and
//!   revocable with the same machinery as API tokens.
//! - Every method repeats the `tenant_id` predicate even though the owner
//!   bypasses RLS. A future refactor that runs these as `tira_app` still cannot
//!   cross a tenant boundary.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::scoped::AuditContext;
use crate::store::{PgStore, StoreError};

/// The minimum needed to verify a password login. Returned only by the owner
/// connection and never rendered.
#[derive(Debug, Clone)]
pub struct LoginCredential {
    pub mailbox_id: Uuid,
    pub tenant_id: Uuid,
    pub address: String,
    pub display_name: Option<String>,
    pub password_hash: String,
}

/// One active credential scoped to a mailbox, for the profile's session list.
#[derive(Debug, Clone)]
pub struct MailboxSession {
    pub id: Uuid,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Real account facts, all from PostgreSQL. `password_set` and `engine_state`
/// are stated honestly rather than inferred.
#[derive(Debug, Clone)]
pub struct MailboxProfile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub address: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub quota_bytes: i64,
    pub used_bytes: i64,
    pub message_count: i64,
    pub password_set: bool,
    pub engine_state: String,
}

impl PgStore {
    /// Resolves an address to its credential row. The address is normalised by
    /// the caller; the schema stores `local_part` and domain names lowercase.
    ///
    /// The predicate is sargable: `d.name = $1 AND m.local_part = $2` is served
    /// by the unique indexes `domains_name_key(name)` and
    /// `mailboxes_address_key(domain_id, local_part)`. Matching the concatenated
    /// `local_part || '@' || name` instead forced a sequential scan of both
    /// tables on every login. Domain names never contain '@', so splitting on
    /// the last one is unambiguous even for a quoted local part; an address
    /// without '@' cannot match any row and resolves to `None`.
    pub async fn credential_by_address(
        &self,
        address: &str,
    ) -> Result<Option<LoginCredential>, StoreError> {
        let Some((local_part, domain)) = address.rsplit_once('@') else {
            return Ok(None);
        };
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT m.id, m.tenant_id,
                        m.local_part || '@' || d.name AS address,
                        m.display_name, c.password_hash
                   FROM mailboxes m
                   JOIN domains d ON d.id = m.domain_id
                   JOIN mailbox_credentials c ON c.mailbox_id = m.id
                  WHERE d.name = $1 AND m.local_part = $2",
                &[&domain, &local_part],
            )
            .await?;

        Ok(row.map(|row| LoginCredential {
            mailbox_id: row.get(0),
            tenant_id: row.get(1),
            address: row.get(2),
            display_name: row.get(3),
            password_hash: row.get(4),
        }))
    }

    /// Returns the stored PHC string for one mailbox. Used when changing a
    /// password and the current password must be proven first.
    pub async fn password_hash(
        &self,
        mailbox_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<String>, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT c.password_hash
                   FROM mailbox_credentials c
                   JOIN mailboxes m ON m.id = c.mailbox_id
                  WHERE c.mailbox_id = $1 AND m.tenant_id = $2",
                &[&mailbox_id, &tenant_id],
            )
            .await?;
        Ok(row.map(|row| row.get(0)))
    }

    /// Creates or replaces the credential. The caller has already hashed the
    /// password; the database CHECK rejects anything that is not Argon2id PHC.
    pub async fn set_password(
        &self,
        mailbox_id: Uuid,
        tenant_id: Uuid,
        password_hash: &str,
    ) -> Result<(), StoreError> {
        let client = self.client().await?;
        let changed = client
            .execute(
                "INSERT INTO mailbox_credentials (mailbox_id, password_hash)
                 SELECT m.id, $3
                   FROM mailboxes m
                  WHERE m.id = $1 AND m.tenant_id = $2
                 ON CONFLICT (mailbox_id) DO UPDATE
                    SET password_hash = EXCLUDED.password_hash,
                        updated_at = now()",
                &[&mailbox_id, &tenant_id, &password_hash],
            )
            .await?;

        if changed == 0 {
            return Err(StoreError::NotFound(format!("mailbox {mailbox_id}")));
        }
        Ok(())
    }

    /// Updates the optional display label. An empty string clears it. Returns
    /// `false` when the mailbox is not visible to this tenant.
    pub async fn set_display_name(
        &self,
        mailbox_id: Uuid,
        tenant_id: Uuid,
        display_name: Option<&str>,
    ) -> Result<bool, StoreError> {
        let client = self.client().await?;
        let changed = client
            .execute(
                "UPDATE mailboxes SET display_name = $3
                  WHERE id = $1 AND tenant_id = $2",
                &[&mailbox_id, &tenant_id, &display_name],
            )
            .await?;
        Ok(changed == 1)
    }

    /// Loads the profile facts in one owner query. `None` means "not this
    /// tenant's mailbox"; callers must render 404, not 403.
    pub async fn mailbox_profile(
        &self,
        mailbox_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<MailboxProfile>, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT m.id, m.tenant_id,
                        m.local_part || '@' || d.name AS address,
                        m.display_name, m.created_at, m.quota_bytes, m.used_bytes,
                        (SELECT count(*) FROM messages msg WHERE msg.mailbox_id = m.id),
                        EXISTS (SELECT 1 FROM mailbox_credentials c WHERE c.mailbox_id = m.id),
                        m.engine_state
                   FROM mailboxes m
                   JOIN domains d ON d.id = m.domain_id
                  WHERE m.id = $1 AND m.tenant_id = $2",
                &[&mailbox_id, &tenant_id],
            )
            .await?;

        Ok(row.map(|row| MailboxProfile {
            id: row.get(0),
            tenant_id: row.get(1),
            address: row.get(2),
            display_name: row.get(3),
            created_at: row.get(4),
            quota_bytes: row.get(5),
            used_bytes: row.get(6),
            message_count: row.get(7),
            password_set: row.get(8),
            engine_state: row.get(9),
        }))
    }

    /// Active credentials scoped to one mailbox, newest first. Tenant-wide
    /// tokens are intentionally absent: they are not this mailbox's sessions.
    pub async fn list_mailbox_sessions(
        &self,
        mailbox_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<MailboxSession>, StoreError> {
        let client = self.client().await?;
        let rows = client
            .query(
                "SELECT id, label, created_at, last_used_at, expires_at
                   FROM api_tokens
                  WHERE tenant_id = $1
                    AND mailbox_id = $2
                    AND revoked_at IS NULL
                    AND (expires_at IS NULL OR expires_at > now())
                  ORDER BY created_at DESC",
                &[&tenant_id, &mailbox_id],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|row| MailboxSession {
                id: row.get(0),
                label: row.get(1),
                created_at: row.get(2),
                last_used_at: row.get(3),
                expires_at: row.get(4),
            })
            .collect())
    }

    /// Revokes one credential only if it belongs to this tenant and mailbox.
    /// Returns `false` for "not yours or already revoked", so the caller can
    /// answer 404 without revealing which.
    pub async fn revoke_mailbox_session(
        &self,
        token_id: Uuid,
        mailbox_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<bool, StoreError> {
        let client = self.client().await?;
        let changed = client
            .execute(
                "UPDATE api_tokens SET revoked_at = now()
                  WHERE id = $1
                    AND tenant_id = $2
                    AND mailbox_id = $3
                    AND revoked_at IS NULL",
                &[&token_id, &tenant_id, &mailbox_id],
            )
            .await?;
        Ok(changed == 1)
    }

    /// Revokes every mailbox-scoped credential except the current session.
    /// Returns the number revoked.
    pub async fn revoke_other_mailbox_sessions(
        &self,
        current_token_id: Uuid,
        mailbox_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<u64, StoreError> {
        let client = self.client().await?;
        let changed = client
            .execute(
                "UPDATE api_tokens SET revoked_at = now()
                  WHERE tenant_id = $1
                    AND mailbox_id = $2
                    AND id <> $3
                    AND revoked_at IS NULL",
                &[&tenant_id, &mailbox_id, &current_token_id],
            )
            .await?;
        Ok(changed)
    }

    /// Appends an authentication event to the audit trail. `tenant_id` is
    /// `None` for a failed login against an address we do not know. The owner
    /// connection is used because login happens before a tenant context.
    pub async fn record_auth_event(
        &self,
        tenant_id: Option<Uuid>,
        audit: &AuditContext,
        action: &str,
        object: Option<&str>,
        result: &str,
        metadata: serde_json::Value,
    ) -> Result<(), StoreError> {
        let client = self.client().await?;
        client
            .execute(
                "INSERT INTO audit_log
                     (tenant_id, actor, action, object, result,
                      request_id, ip, user_agent, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb)",
                &[
                    &tenant_id,
                    &audit.actor,
                    &action,
                    &object,
                    &result,
                    &audit.request_id,
                    &audit.ip,
                    &audit.user_agent,
                    &metadata,
                ],
            )
            .await?;
        Ok(())
    }
}
