//! Control-plane side of the identity bridge to Stalwart.
//!
//! A mailbox row is the source of truth for *who exists*. The engine is the
//! source of truth for *how mail is stored*. These helpers move a mailbox
//! through the only legal path between the two:
//!
//! ```text
//! create_mailbox()  ->  pending
//!   provision succeeds -> active (engine_id recorded)
//!   provision fails    -> failed  (reason recorded, retry allowed)
//!   admin disables     -> disabled
//! ```
//!
//! No password is stored here. The one-time secret is handed to the engine by
//! `tira-provision` and then dropped; see `crates/tira-provision`.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::{PgStore, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Pending,
    Active,
    Disabled,
    Failed,
}

impl EngineState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "disabled" => Ok(Self::Disabled),
            "failed" => Ok(Self::Failed),
            other => Err(StoreError::InvalidState(format!(
                "unknown engine_state {other:?}"
            ))),
        }
    }
}

/// Everything the provisioning client needs, and nothing it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMailbox {
    pub mailbox_id: Uuid,
    pub tenant_id: Uuid,
    pub domain_id: Uuid,
    /// Lowercase domain name, e.g. `uca.ma`.
    pub domain: String,
    /// Lowercase local part, e.g. `aicha`.
    pub local_part: String,
    pub display_name: Option<String>,
    pub quota_bytes: i64,
    pub engine_state: EngineState,
    pub engine_id: Option<String>,
}

impl PendingMailbox {
    pub fn address(&self) -> String {
        format!("{}@{}", self.local_part, self.domain)
    }
}

impl PgStore {
    /// Loads one mailbox with the fields the engine needs.
    ///
    /// Returns [`StoreError::NotFound`] for an unknown id so the CLI can exit
    /// with a clear message instead of provisioning a ghost.
    pub async fn provision_target(&self, mailbox_id: Uuid) -> Result<PendingMailbox, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT m.id, m.tenant_id, m.domain_id, d.name, m.local_part,
                        m.display_name, m.quota_bytes, m.engine_state, m.engine_id
                   FROM mailboxes m
                   JOIN domains d ON d.id = m.domain_id
                  WHERE m.id = $1",
                &[&mailbox_id],
            )
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("mailbox {mailbox_id}")))?;

        let state: String = row.get(7);
        Ok(PendingMailbox {
            mailbox_id: row.get(0),
            tenant_id: row.get(1),
            domain_id: row.get(2),
            domain: row.get(3),
            local_part: row.get(4),
            display_name: row.get(5),
            quota_bytes: row.get(6),
            engine_state: EngineState::from_db(&state)?,
            engine_id: row.get(8),
        })
    }

    /// Marks a mailbox as provisioned and records the engine's own id.
    ///
    /// The `WHERE engine_state <> 'active'` guard makes the call idempotent:
    /// re-running the bridge cannot silently overwrite a different engine id.
    pub async fn mark_mailbox_active(
        &self,
        mailbox_id: Uuid,
        engine_id: &str,
    ) -> Result<(), StoreError> {
        let client = self.client().await?;
        let changed = client
            .execute(
                "UPDATE mailboxes
                    SET engine_id = $2,
                        engine_state = 'active',
                        engine_error = NULL,
                        engine_synced_at = now()
                  WHERE id = $1
                    AND engine_state <> 'active'",
                &[&mailbox_id, &engine_id],
            )
            .await?;
        if changed == 0 {
            let current = self.provision_target(mailbox_id).await?;
            return Err(StoreError::Conflict(format!(
                "mailbox {mailbox_id} is already {}, refusing to relink it to {engine_id}",
                current.engine_state.as_str()
            )));
        }
        Ok(())
    }

    /// Records a failed attempt. The row stays retryable; the old `engine_id`
    /// is left untouched on purpose.
    pub async fn mark_mailbox_failed(
        &self,
        mailbox_id: Uuid,
        reason: &str,
    ) -> Result<(), StoreError> {
        let client = self.client().await?;
        let reason = truncate(reason, 1000);
        client
            .execute(
                "UPDATE mailboxes
                    SET engine_state = 'failed',
                        engine_error = $2,
                        engine_synced_at = now()
                  WHERE id = $1
                    AND engine_state <> 'active'",
                &[&mailbox_id, &reason],
            )
            .await?;
        Ok(())
    }

    /// Disables an already-linked mailbox. Mail is kept; access is revoked.
    pub async fn mark_mailbox_disabled(&self, mailbox_id: Uuid) -> Result<(), StoreError> {
        let client = self.client().await?;
        let changed = client
            .execute(
                "UPDATE mailboxes
                    SET engine_state = 'disabled',
                        engine_synced_at = now()
                  WHERE id = $1",
                &[&mailbox_id],
            )
            .await?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("mailbox {mailbox_id}")));
        }
        Ok(())
    }

    /// Mailboxes that are not yet active, oldest first. Used by reconciliation.
    pub async fn unprovisioned_mailboxes(
        &self,
        limit: i64,
    ) -> Result<Vec<PendingMailbox>, StoreError> {
        let client = self.client().await?;
        let rows = client
            .query(
                "SELECT m.id, m.tenant_id, m.domain_id, d.name, m.local_part,
                        m.display_name, m.quota_bytes, m.engine_state, m.engine_id
                   FROM mailboxes m
                   JOIN domains d ON d.id = m.domain_id
                  WHERE m.engine_state <> 'active'
                  ORDER BY m.created_at
                  LIMIT $1",
                &[&limit.clamp(1, 1000)],
            )
            .await?;

        rows.into_iter()
            .map(|row| {
                let state: String = row.get(7);
                Ok(PendingMailbox {
                    mailbox_id: row.get(0),
                    tenant_id: row.get(1),
                    domain_id: row.get(2),
                    domain: row.get(3),
                    local_part: row.get(4),
                    display_name: row.get(5),
                    quota_bytes: row.get(6),
                    engine_state: EngineState::from_db(&state)?,
                    engine_id: row.get(8),
                })
            })
            .collect()
    }

    /// Every linked mailbox, oldest first. The reconciliation pass compares this
    /// list with a snapshot of the engine, so it must contain only rows the
    /// engine is expected to know about.
    pub async fn active_mailboxes(&self, limit: i64) -> Result<Vec<PendingMailbox>, StoreError> {
        let client = self.client().await?;
        let rows = client
            .query(
                "SELECT m.id, m.tenant_id, m.domain_id, d.name, m.local_part,
                        m.display_name, m.quota_bytes, m.engine_state, m.engine_id
                   FROM mailboxes m
                   JOIN domains d ON d.id = m.domain_id
                  WHERE m.engine_state = 'active'
                  ORDER BY m.created_at
                  LIMIT $1",
                &[&limit.clamp(1, 100_000)],
            )
            .await?;

        rows.into_iter()
            .map(|row| {
                let state: String = row.get(7);
                Ok(PendingMailbox {
                    mailbox_id: row.get(0),
                    tenant_id: row.get(1),
                    domain_id: row.get(2),
                    domain: row.get(3),
                    local_part: row.get(4),
                    display_name: row.get(5),
                    quota_bytes: row.get(6),
                    engine_state: EngineState::from_db(&state)?,
                    engine_id: row.get(8),
                })
            })
            .collect()
    }

    /// Records that an `active` row was checked against the engine and matched.
    ///
    /// This is the only write a healthy reconciliation performs, and it is what
    /// makes `engine_synced_at` meaningful: its age is the age of the last
    /// *verification*, not the age of the last provisioning attempt.
    pub async fn mark_mailbox_verified(&self, mailbox_id: Uuid) -> Result<(), StoreError> {
        let client = self.client().await?;
        client
            .execute(
                "UPDATE mailboxes
                    SET engine_synced_at = now()
                  WHERE id = $1
                    AND engine_state = 'active'",
                &[&mailbox_id],
            )
            .await?;
        Ok(())
    }

    /// Demotes an `active` row to `failed` because the engine no longer matches
    /// the control plane. `engine_id` is kept on purpose: an operator needs to
    /// see what the link *used* to be before deciding whether to re-provision.
    ///
    /// Returns `false` when the row was no longer `active` (concurrent change),
    /// which is not an error for a reconciliation pass.
    pub async fn mark_mailbox_drifted(
        &self,
        mailbox_id: Uuid,
        reason: &str,
    ) -> Result<bool, StoreError> {
        let client = self.client().await?;
        let reason = truncate(reason, 1000);
        let changed = client
            .execute(
                "UPDATE mailboxes
                    SET engine_state = 'failed',
                        engine_error = $2,
                        engine_synced_at = now()
                  WHERE id = $1
                    AND engine_state = 'active'",
                &[&mailbox_id, &reason],
            )
            .await?;
        Ok(changed > 0)
    }

    /// When was this mailbox last reconciled with the engine?
    pub async fn engine_synced_at(
        &self,
        mailbox_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT engine_synced_at FROM mailboxes WHERE id = $1",
                &[&mailbox_id],
            )
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("mailbox {mailbox_id}")))?;
        Ok(row.get(0))
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrips_through_text() {
        for state in [
            EngineState::Pending,
            EngineState::Active,
            EngineState::Disabled,
            EngineState::Failed,
        ] {
            assert_eq!(EngineState::from_db(state.as_str()).unwrap(), state);
        }
    }

    #[test]
    fn unknown_state_is_rejected() {
        assert!(matches!(
            EngineState::from_db("half-active"),
            Err(StoreError::InvalidState(_))
        ));
    }

    #[test]
    fn truncate_respects_utf8_boundaries() {
        let text = "مستخدم غير صالح";
        let cut = truncate(text, 5);
        assert!(cut.ends_with('…'));
        assert!(cut.len() <= 5 + '…'.len_utf8());
    }
}
