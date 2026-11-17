//! The only write path from the control plane to the engine.
//!
//! Every call is idempotent at the engine level (`upsert` + `matchOn`) and
//! resumable at the database level (`engine_state`). If the process dies after
//! the engine accepted the account but before PostgreSQL recorded the id, the
//! next run re-upserts the same account and only then records it.

use tira_store::{EngineState, PgStore};
use uuid::Uuid;

use crate::engine::{EngineError, MailEngine};
use crate::plan::{account_upsert, domain_upsert};

pub const MIN_SECRET_LEN: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    #[error("store error: {0}")]
    Store(#[from] tira_store::StoreError),
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("mailbox {0} is disabled; re-enable it before provisioning")]
    Disabled(Uuid),
    #[error("password must be at least {min} characters")]
    WeakSecret { min: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionOutcome {
    pub mailbox_id: Uuid,
    pub address: String,
    pub engine_id: String,
    /// True when the mailbox was already linked; no engine call was made.
    pub already_active: bool,
}

/// How one `active` mailbox compares with the engine snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drift {
    /// The engine still knows the account and its id matches the database.
    InSync,
    /// The database says `active`, but the engine has no such account.
    MissingFromEngine,
    /// The account exists, but under an id the database does not know.
    EngineIdMismatch,
    /// The domain itself is gone from the engine.
    DomainMissing,
}

impl Drift {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InSync => "in-sync",
            Self::MissingFromEngine => "missing-from-engine",
            Self::EngineIdMismatch => "engine-id-mismatch",
            Self::DomainMissing => "domain-missing",
        }
    }

    pub fn is_drift(self) -> bool {
        self != Self::InSync
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileEntry {
    pub mailbox_id: Uuid,
    pub address: String,
    /// The id recorded in PostgreSQL, if any.
    pub expected_engine_id: Option<String>,
    /// The id the engine reports now, if any.
    pub actual_engine_id: Option<String>,
    pub drift: Drift,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub checked: usize,
    pub in_sync: usize,
    pub drifted: usize,
    pub entries: Vec<ReconcileEntry>,
}

impl ReconcileReport {
    pub fn is_healthy(&self) -> bool {
        self.drifted == 0
    }

    pub fn drifted_entries(&self) -> impl Iterator<Item = &ReconcileEntry> {
        self.entries.iter().filter(|entry| entry.drift.is_drift())
    }
}

#[derive(Debug, Clone)]
pub struct Provisioner<E: MailEngine> {
    store: PgStore,
    engine: E,
}

impl<E: MailEngine> Provisioner<E> {
    pub fn new(store: PgStore, engine: E) -> Self {
        Self { store, engine }
    }

    /// Provisions one mailbox with a one-time password.
    ///
    /// The password is used to build the plan, handed to the engine, and then
    /// dropped with the plan. It is never written to PostgreSQL.
    pub async fn provision(
        &self,
        mailbox_id: Uuid,
        secret: &str,
    ) -> Result<ProvisionOutcome, ProvisionError> {
        let target = self.store.provision_target(mailbox_id).await?;

        match target.engine_state {
            EngineState::Active => {
                return Ok(ProvisionOutcome {
                    mailbox_id,
                    address: target.address(),
                    engine_id: target.engine_id.unwrap_or_default(),
                    already_active: true,
                });
            }
            EngineState::Disabled => return Err(ProvisionError::Disabled(mailbox_id)),
            EngineState::Pending | EngineState::Failed => {}
        }

        if secret.chars().count() < MIN_SECRET_LEN {
            return Err(ProvisionError::WeakSecret {
                min: MIN_SECRET_LEN,
            });
        }

        let plan = [domain_upsert(&target), account_upsert(&target, secret)];

        if let Err(error) = self.engine.apply(&plan).await {
            let _ = self
                .store
                .mark_mailbox_failed(mailbox_id, &error.to_string())
                .await;
            return Err(error.into());
        }

        let engine_id = match self
            .engine
            .lookup_account(&target.local_part, &target.domain)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                let error = EngineError::AccountNotFound(target.address());
                let _ = self
                    .store
                    .mark_mailbox_failed(mailbox_id, &error.to_string())
                    .await;
                return Err(error.into());
            }
            Err(error) => {
                let _ = self
                    .store
                    .mark_mailbox_failed(mailbox_id, &error.to_string())
                    .await;
                return Err(error.into());
            }
        };

        self.store
            .mark_mailbox_active(mailbox_id, &engine_id)
            .await?;

        Ok(ProvisionOutcome {
            mailbox_id,
            address: target.address(),
            engine_id,
            already_active: false,
        })
    }

    /// Lists mailboxes the engine does not know about yet.
    pub async fn pending(
        &self,
        limit: i64,
    ) -> Result<Vec<tira_store::PendingMailbox>, ProvisionError> {
        Ok(self.store.unprovisioned_mailboxes(limit).await?)
    }

    /// Compares every `active` mailbox with a snapshot of the engine.
    ///
    /// Two engine queries total, regardless of mailbox count. When `apply` is
    /// true, drifted rows are demoted to `failed` with a reason so they show up
    /// in `--list-pending` and can be re-provisioned deliberately; when it is
    /// false the pass is read-only.
    ///
    /// Drift is not repaired automatically on purpose. A missing engine account
    /// may mean a deleted user, a failed restore, or a wrong management URL —
    /// re-creating the account without a human decision would hide all three.
    pub async fn reconcile(
        &self,
        apply: bool,
        limit: i64,
    ) -> Result<ReconcileReport, ProvisionError> {
        let snapshot = self.engine.snapshot().await?;
        let mailboxes = self.store.active_mailboxes(limit).await?;

        let mut report = ReconcileReport {
            checked: mailboxes.len(),
            ..ReconcileReport::default()
        };

        for mailbox in mailboxes {
            let address = mailbox.address();
            let key = (mailbox.local_part.clone(), mailbox.domain.clone());

            let (drift, actual) = if !snapshot.domains.contains_key(&mailbox.domain) {
                (Drift::DomainMissing, None)
            } else {
                match snapshot.accounts.get(&key) {
                    None => (Drift::MissingFromEngine, None),
                    Some(actual) if mailbox.engine_id.as_deref() == Some(actual.as_str()) => {
                        (Drift::InSync, Some(actual.clone()))
                    }
                    Some(actual) => (Drift::EngineIdMismatch, Some(actual.clone())),
                }
            };

            if drift.is_drift() {
                report.drifted += 1;
                if apply {
                    let reason = format!(
                        "reconcile: {} for {address} (recorded {}, engine {})",
                        drift.as_str(),
                        mailbox.engine_id.as_deref().unwrap_or("none"),
                        actual.as_deref().unwrap_or("none"),
                    );
                    self.store
                        .mark_mailbox_drifted(mailbox.mailbox_id, &reason)
                        .await?;
                }
            } else {
                report.in_sync += 1;
                self.store.mark_mailbox_verified(mailbox.mailbox_id).await?;
            }

            report.entries.push(ReconcileEntry {
                mailbox_id: mailbox.mailbox_id,
                address,
                expected_engine_id: mailbox.engine_id,
                actual_engine_id: actual,
                drift,
            });
        }

        Ok(report)
    }
}
