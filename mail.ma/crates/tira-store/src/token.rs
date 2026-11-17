//! Bearer tokens for the read API.
//!
//! Design constraints:
//!
//! - **The control plane never stores the secret.** Only `sha256(secret)` is
//!   written, and the plaintext is printed once by `tira-token`.
//! - **One token belongs to exactly one tenant.** A token may additionally be
//!   narrowed to a single mailbox, but it can never cross a tenant boundary.
//! - **Fast hash on purpose.** These are 32 random bytes, not human passwords;
//!   Argon2id would add ~50ms to every HTTP request and buy nothing against a
//!   brute-force that already needs 2^256 guesses.
//! - **Revocation is a row update**, not a redeploy.

use std::fmt;

use chrono::{DateTime, Duration, Utc};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::store::{PgStore, StoreError};

/// All tokens start with this. It lets a leaked string be recognised and lets
/// the middleware reject obvious garbage before touching the database.
pub const TOKEN_PREFIX: &str = "tira_";

/// 256 bits of entropy. 32 bytes -> 64 hex characters after the prefix.
const SECRET_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiToken {
    pub id: Uuid,
    pub tenant_id: Uuid,
    /// `None` = every mailbox in the tenant. `Some(id)` = that mailbox only.
    pub mailbox_id: Option<Uuid>,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Returned exactly once, at creation time.
#[derive(Clone)]
pub struct IssuedToken {
    /// The secret. Print it, hand it over, forget it.
    pub secret: String,
    pub record: ApiToken,
}

/// The secret must never reach a log, a panic message or a `{:?}` dump. Only the
/// non-secret record is printed; the secret is replaced with a fixed marker so
/// an accidental `tracing::debug!(?issued)` cannot leak a live credential.
impl fmt::Debug for IssuedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IssuedToken")
            .field("secret", &"<redacted>")
            .field("record", &self.record)
            .finish()
    }
}

/// A CSPRNG secret: `tira_` + 64 lowercase hex characters.
pub fn new_secret() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    OsRng.fill_bytes(&mut bytes);
    format!("{TOKEN_PREFIX}{}", hex::encode(bytes))
}

pub fn hash_secret(secret: &str) -> [u8; 32] {
    let digest = Sha256::digest(secret.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Parses `Authorization: Bearer <secret>`.
///
/// The scheme is case-insensitive (RFC 7235); the secret is not. Tokens with
/// embedded whitespace are rejected rather than trimmed: a malformed header
/// must not be silently accepted as a different token.
pub fn parse_bearer(header: &str) -> Option<&str> {
    let (scheme, secret) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    if secret.is_empty() || secret.chars().any(char::is_whitespace) {
        return None;
    }
    Some(secret)
}

impl PgStore {
    /// Creates a token. `ttl` of `None` means "does not expire"; the CLI asks
    /// for a finite TTL by default.
    ///
    /// The tenant (and the mailbox, when given) is verified first so a typo
    /// returns `NotFound` instead of a foreign-key error.
    pub async fn issue_token(
        &self,
        tenant_id: Uuid,
        mailbox_id: Option<Uuid>,
        label: &str,
        ttl: Option<Duration>,
    ) -> Result<IssuedToken, StoreError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(StoreError::InvalidToken(
                "token label must not be empty".to_string(),
            ));
        }

        let client = self.client().await?;

        let tenant_exists: bool = client
            .query_opt("SELECT 1 FROM tenants WHERE id = $1", &[&tenant_id])
            .await?
            .is_some();
        if !tenant_exists {
            return Err(StoreError::NotFound(format!("tenant {tenant_id}")));
        }

        if let Some(mailbox_id) = mailbox_id {
            let row = client
                .query_opt(
                    "SELECT tenant_id FROM mailboxes WHERE id = $1",
                    &[&mailbox_id],
                )
                .await?
                .ok_or_else(|| StoreError::NotFound(format!("mailbox {mailbox_id}")))?;
            let owner: Uuid = row.get(0);
            if owner != tenant_id {
                return Err(StoreError::Conflict(format!(
                    "mailbox {mailbox_id} does not belong to tenant {tenant_id}"
                )));
            }
        }

        let secret = new_secret();
        let hash = hash_secret(&secret);
        let id = Uuid::new_v4();
        let now = Utc::now();
        let expires_at = ttl.map(|ttl| now + ttl);

        client
            .execute(
                "INSERT INTO api_tokens
                     (id, tenant_id, mailbox_id, label, token_hash, created_at, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &id,
                    &tenant_id,
                    &mailbox_id,
                    &label,
                    &hash.as_slice(),
                    &now,
                    &expires_at,
                ],
            )
            .await?;

        Ok(IssuedToken {
            secret,
            record: ApiToken {
                id,
                tenant_id,
                mailbox_id,
                label: label.to_string(),
                created_at: now,
                expires_at,
                last_used_at: None,
            },
        })
    }

    /// Resolves a secret to its tenant scope. Returns `None` for unknown,
    /// revoked, or expired tokens — the HTTP layer must not tell them apart.
    ///
    /// All of this happens inside `tira_resolve_token(bytea)`:
    ///
    /// - the hash is looked up in `api_tokens`, a table RLS denies to the
    ///   application role, so the function is `SECURITY DEFINER`;
    /// - `last_used_at` is updated in the same statement, so the read and the
    ///   telemetry cannot disagree or race;
    /// - revoked and expired tokens never match, and the caller cannot learn
    ///   *why* a token was rejected.
    pub async fn resolve_token(&self, secret: &str) -> Result<Option<ApiToken>, StoreError> {
        if !secret.starts_with(TOKEN_PREFIX) {
            return Ok(None);
        }
        let hash = hash_secret(secret);
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT token_id, token_tenant_id, token_mailbox_id, token_label,
                        token_created_at, token_expires_at, token_used_at
                   FROM tira_resolve_token($1)",
                &[&hash.as_slice()],
            )
            .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(ApiToken {
            id: row.get(0),
            tenant_id: row.get(1),
            mailbox_id: row.get(2),
            label: row.get(3),
            created_at: row.get(4),
            expires_at: row.get(5),
            last_used_at: row.get(6),
        }))
    }

    /// Revokes one token. Returns `false` when it does not exist or was already
    /// revoked, so the CLI can report a no-op honestly.
    pub async fn revoke_token(&self, id: Uuid) -> Result<bool, StoreError> {
        let client = self.client().await?;
        let changed = client
            .execute(
                "UPDATE api_tokens SET revoked_at = now()
                  WHERE id = $1 AND revoked_at IS NULL",
                &[&id],
            )
            .await?;
        Ok(changed == 1)
    }

    /// Lists tokens for a tenant. Never returns hashes or secrets.
    pub async fn list_tokens(&self, tenant_id: Uuid) -> Result<Vec<ApiToken>, StoreError> {
        let client = self.client().await?;
        let rows = client
            .query(
                "SELECT id, tenant_id, mailbox_id, label, created_at, expires_at, last_used_at
                   FROM api_tokens
                  WHERE tenant_id = $1
                  ORDER BY created_at DESC",
                &[&tenant_id],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|row| ApiToken {
                id: row.get(0),
                tenant_id: row.get(1),
                mailbox_id: row.get(2),
                label: row.get(3),
                created_at: row.get(4),
                expires_at: row.get(5),
                last_used_at: row.get(6),
            })
            .collect())
    }

    /// Removes token rows that can no longer authenticate anything and whose
    /// death is older than `grace`: expired (`expires_at` in the past) or
    /// revoked (`revoked_at` set). The grace window keeps a freshly-dead row
    /// briefly for debugging and operator listing; after it the row is pure
    /// bloat that would otherwise accumulate forever.
    ///
    /// Deleting a dead token never erases evidence: the access trail lives in
    /// `audit_log`, whose `actor` is text (`token:<uuid>`), not a foreign key.
    /// Live tokens (unexpired, unrevoked) are never touched.
    ///
    /// Returns the number of rows removed.
    pub async fn purge_dead_tokens(&self, grace: Duration) -> Result<u64, StoreError> {
        let cutoff = Utc::now() - grace;
        let client = self.client().await?;
        let removed = client
            .execute(
                "DELETE FROM api_tokens
                  WHERE (expires_at IS NOT NULL AND expires_at < $1)
                     OR (revoked_at IS NOT NULL AND revoked_at < $1)",
                &[&cutoff],
            )
            .await?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_prefixed_and_high_entropy() {
        let a = new_secret();
        let b = new_secret();
        assert!(a.starts_with(TOKEN_PREFIX));
        assert_eq!(a.len(), TOKEN_PREFIX.len() + SECRET_BYTES * 2);
        assert!(a[TOKEN_PREFIX.len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn hashing_is_deterministic_and_sensitive() {
        let secret = new_secret();
        assert_eq!(hash_secret(&secret), hash_secret(&secret));
        assert_ne!(hash_secret(&secret), hash_secret(&new_secret()));
        assert_eq!(hash_secret(&secret).len(), 32);
    }

    #[test]
    fn bearer_parsing_is_strict() {
        let secret = new_secret();
        assert_eq!(
            parse_bearer(&format!("Bearer {secret}")),
            Some(secret.as_str())
        );
        assert_eq!(
            parse_bearer(&format!("bearer {secret}")),
            Some(secret.as_str())
        );
        assert_eq!(parse_bearer("Basic abc"), None);
        assert_eq!(parse_bearer("Bearer"), None);
        assert_eq!(parse_bearer("Bearer "), None);
        assert_eq!(parse_bearer("Bearer  two"), None);
        assert_eq!(parse_bearer(&format!("Bearer {secret} extra")), None);
    }

    #[test]
    fn issued_token_debug_never_prints_the_secret() {
        let secret = new_secret();
        let issued = IssuedToken {
            secret: secret.clone(),
            record: ApiToken {
                id: Uuid::nil(),
                tenant_id: Uuid::nil(),
                mailbox_id: None,
                label: "redaction-test".to_string(),
                created_at: Utc::now(),
                expires_at: None,
                last_used_at: None,
            },
        };
        let rendered = format!("{issued:?}");
        assert!(
            !rendered.contains(&secret),
            "Debug leaked the secret: {rendered}"
        );
        assert!(
            rendered.contains("<redacted>"),
            "secret field must be masked: {rendered}"
        );
        // The non-secret record stays useful for diagnostics.
        assert!(
            rendered.contains("redaction-test"),
            "record label should remain: {rendered}"
        );
    }
}
