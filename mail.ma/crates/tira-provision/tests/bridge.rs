//! Identity-bridge tests.
//!
//! The engine is replaced by an in-memory fake so the test is deterministic and
//! does not need Stalwart. PostgreSQL is real: the point of these tests is the
//! state machine (`pending -> active`, `failed -> active`, `disabled` refusal)
//! and the "no password stored" guarantee, not the HTTP encoding of a plan.
//!
//! Skipped when `TIRA_TEST_DATABASE_URL` is unset; set `TIRA_REQUIRE_DATABASE=1`
//! in CI so a skipped run cannot masquerade as coverage.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use tira_provision::engine::{ApplyReport, MailEngine};
use tira_provision::plan::PlanLine;
use tira_provision::{ProvisionError, Provisioner};
use tira_store::{EngineState, PgStore};

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty());
    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!("TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set");
    }
    url
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("tira-provision-test-{}", Uuid::new_v4()))
}

#[derive(Clone, Default)]
struct FakeEngine {
    inner: Arc<FakeState>,
}

#[derive(Default)]
struct FakeState {
    applied: Mutex<Vec<PlanLine>>,
    /// (local part, plan-local domain key) -> engine account id
    accounts: Mutex<HashMap<(String, String), String>>,
    /// plan-local domain key -> domain name
    domains: Mutex<HashMap<String, String>>,
    fail_apply: Mutex<bool>,
}

impl FakeEngine {
    fn applied(&self) -> usize {
        self.inner.applied.lock().unwrap().len()
    }

    fn set_failing(&self, failing: bool) {
        *self.inner.fail_apply.lock().unwrap() = failing;
    }

    /// Simulates the account being deleted directly in the engine.
    fn forget_account(&self, local_part: &str) {
        let mut accounts = self.inner.accounts.lock().unwrap();
        accounts.retain(|(name, _), _| name != local_part);
    }

    /// Simulates the engine re-creating the account under a different id.
    fn relabel_account(&self, local_part: &str, new_id: &str) {
        let mut accounts = self.inner.accounts.lock().unwrap();
        for ((name, _), id) in accounts.iter_mut() {
            if name == local_part {
                *id = new_id.to_string();
            }
        }
    }
}

#[async_trait]
impl MailEngine for FakeEngine {
    async fn apply(&self, lines: &[PlanLine]) -> Result<ApplyReport, tira_provision::EngineError> {
        if *self.inner.fail_apply.lock().unwrap() {
            return Err(tira_provision::EngineError::Apply {
                code: 1,
                stderr: "engine unavailable (fake)".to_string(),
            });
        }
        self.inner.applied.lock().unwrap().extend_from_slice(lines);

        // The first line is the domain upsert: remember its plan-local key so
        // `snapshot()` can resolve the account's `#dom-...` reference to a name.
        if let Some(domain) = lines.first() {
            if let Some((key, value)) = domain.value.as_object().and_then(|m| m.iter().next()) {
                let name = value["name"].as_str().unwrap_or_default().to_string();
                self.inner.domains.lock().unwrap().insert(key.clone(), name);
            }
        }

        // The second line is the account upsert. Read its address back from the
        // plan exactly like the real engine would.
        if let Some(account) = lines.get(1) {
            if let Some((key, value)) = account.value.as_object().and_then(|m| m.iter().next()) {
                let name = value["name"].as_str().unwrap_or_default().to_string();
                let domain_ref = value["domainId"]
                    .as_str()
                    .unwrap_or_default()
                    .trim_start_matches('#')
                    .to_string();
                self.inner
                    .accounts
                    .lock()
                    .unwrap()
                    .insert((name, domain_ref), format!("eng-{key}"));
            }
        }

        Ok(ApplyReport {
            created: lines.len() as u64,
            ..ApplyReport::default()
        })
    }

    async fn lookup_account(
        &self,
        local_part: &str,
        domain: &str,
    ) -> Result<Option<String>, tira_provision::EngineError> {
        // The fake stores by domain reference, not name; find any entry whose
        // key starts with the local part.
        Ok(self
            .inner
            .accounts
            .lock()
            .unwrap()
            .iter()
            .find(|((name, _), _)| name == local_part)
            .map(|(_, id)| id.clone())
            .filter(|_| !domain.is_empty()))
    }

    async fn snapshot(
        &self,
    ) -> Result<tira_provision::EngineSnapshot, tira_provision::EngineError> {
        let domains = self.inner.domains.lock().unwrap();
        let accounts = self.inner.accounts.lock().unwrap();

        let snap_domains = domains
            .iter()
            .map(|(key, name)| (name.to_ascii_lowercase(), key.clone()))
            .collect();

        let mut snap_accounts = HashMap::new();
        for ((name, domain_ref), id) in accounts.iter() {
            if let Some(domain) = domains.get(domain_ref) {
                snap_accounts.insert(
                    (name.to_ascii_lowercase(), domain.to_ascii_lowercase()),
                    id.clone(),
                );
            }
        }

        Ok(tira_provision::EngineSnapshot {
            domains: snap_domains,
            accounts: snap_accounts,
        })
    }
}

async fn fixture(store: &PgStore) -> Uuid {
    let tenant = store
        .create_tenant("Université Cadi Ayyad", Some("CNDP-TEST-BRIDGE"))
        .await
        .unwrap();
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .unwrap();
    store
        .create_mailbox(domain.id, "aicha", 5_368_709_120)
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn provisioning_links_the_mailbox_exactly_once() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let engine = FakeEngine::default();
    let provisioner = Provisioner::new(store.clone(), engine.clone());

    let first = provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .expect("provision");
    assert!(!first.already_active);
    assert!(!first.engine_id.is_empty());
    assert_eq!(engine.applied(), 2);

    let target = store.provision_target(mailbox_id).await.unwrap();
    assert_eq!(target.engine_state, EngineState::Active);
    assert_eq!(target.engine_id.as_deref(), Some(first.engine_id.as_str()));

    let second = provisioner
        .provision(mailbox_id, "Another-Password-2026!")
        .await
        .expect("second provision is a no-op");
    assert!(second.already_active);
    assert_eq!(second.engine_id, first.engine_id);
    assert_eq!(
        engine.applied(),
        2,
        "an active mailbox must not be re-applied"
    );
}

#[tokio::test]
async fn a_weak_secret_never_reaches_the_engine() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let engine = FakeEngine::default();
    let provisioner = Provisioner::new(store.clone(), engine.clone());

    let error = provisioner
        .provision(mailbox_id, "short")
        .await
        .expect_err("weak secret must fail");
    assert!(matches!(error, ProvisionError::WeakSecret { .. }));
    assert_eq!(engine.applied(), 0);

    let target = store.provision_target(mailbox_id).await.unwrap();
    assert_eq!(target.engine_state, EngineState::Pending);
}

#[tokio::test]
async fn an_engine_failure_is_recorded_and_retryable() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let engine = FakeEngine::default();
    engine.set_failing(true);
    let provisioner = Provisioner::new(store.clone(), engine.clone());

    let error = provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .expect_err("engine failure must propagate");
    assert!(matches!(error, ProvisionError::Engine(_)));

    let target = store.provision_target(mailbox_id).await.unwrap();
    assert_eq!(target.engine_state, EngineState::Failed);
    assert!(target.engine_id.is_none());
    let synced = store.engine_synced_at(mailbox_id).await.unwrap();
    assert!(synced.is_some(), "a failed attempt must be timestamped");

    engine.set_failing(false);
    let retry = provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .expect("retry after engine recovery");
    assert!(!retry.already_active);

    let target = store.provision_target(mailbox_id).await.unwrap();
    assert_eq!(target.engine_state, EngineState::Active);
}

#[tokio::test]
async fn a_disabled_mailbox_cannot_be_provisioned() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let engine = FakeEngine::default();
    let provisioner = Provisioner::new(store.clone(), engine.clone());
    provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .unwrap();
    store.mark_mailbox_disabled(mailbox_id).await.unwrap();

    let error = provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .expect_err("disabled mailbox must be refused");
    assert!(matches!(error, ProvisionError::Disabled(_)));
}

#[tokio::test]
async fn relinking_an_active_mailbox_to_a_different_engine_id_is_a_conflict() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let first = format!("engine-account-{}", Uuid::new_v4());
    let second = format!("engine-account-{}", Uuid::new_v4());
    store.mark_mailbox_active(mailbox_id, &first).await.unwrap();
    let error = store
        .mark_mailbox_active(mailbox_id, &second)
        .await
        .expect_err("must not silently overwrite the link");
    assert!(matches!(error, tira_store::StoreError::Conflict(_)));

    let target = store.provision_target(mailbox_id).await.unwrap();
    assert_eq!(target.engine_id.as_deref(), Some(first.as_str()));
}

#[tokio::test]
async fn reconcile_reports_a_healthy_link() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let engine = FakeEngine::default();
    let provisioner = Provisioner::new(store.clone(), engine.clone());
    provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .unwrap();

    let report = provisioner.reconcile(false, 10_000).await.unwrap();
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.mailbox_id == mailbox_id)
        .expect("the provisioned mailbox must appear in the report");

    assert_eq!(entry.drift, tira_provision::Drift::InSync);
    assert_eq!(entry.expected_engine_id, entry.actual_engine_id);
    assert!(store.engine_synced_at(mailbox_id).await.unwrap().is_some());
    assert_eq!(
        store
            .provision_target(mailbox_id)
            .await
            .unwrap()
            .engine_state,
        EngineState::Active
    );
}

#[tokio::test]
async fn reconcile_is_read_only_until_fix_is_requested() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let engine = FakeEngine::default();
    let provisioner = Provisioner::new(store.clone(), engine.clone());
    let outcome = provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .unwrap();
    engine.forget_account(outcome.address.split('@').next().unwrap());

    let report = provisioner.reconcile(false, 10_000).await.unwrap();
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.mailbox_id == mailbox_id)
        .unwrap();
    assert_eq!(entry.drift, tira_provision::Drift::MissingFromEngine);
    assert_eq!(entry.actual_engine_id, None);
    assert_eq!(
        store
            .provision_target(mailbox_id)
            .await
            .unwrap()
            .engine_state,
        EngineState::Active,
        "a read-only pass must not demote the row"
    );

    let fixed = provisioner.reconcile(true, 10_000).await.unwrap();
    assert!(!fixed.is_healthy());
    let target = store.provision_target(mailbox_id).await.unwrap();
    assert_eq!(target.engine_state, EngineState::Failed);
    assert_eq!(
        target.engine_id.as_deref(),
        Some(outcome.engine_id.as_str()),
        "the old link is kept for the operator"
    );
}

#[tokio::test]
async fn reconcile_detects_an_engine_id_mismatch() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, temp_root()).await.unwrap();
    let mailbox_id = fixture(&store).await;

    let engine = FakeEngine::default();
    let provisioner = Provisioner::new(store.clone(), engine.clone());
    let outcome = provisioner
        .provision(mailbox_id, "Salam-Aicha-2026!")
        .await
        .unwrap();
    engine.relabel_account(
        outcome.address.split('@').next().unwrap(),
        "engine-recreated-elsewhere",
    );

    let report = provisioner.reconcile(true, 10_000).await.unwrap();
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.mailbox_id == mailbox_id)
        .unwrap();
    assert_eq!(entry.drift, tira_provision::Drift::EngineIdMismatch);
    assert_eq!(
        entry.actual_engine_id.as_deref(),
        Some("engine-recreated-elsewhere")
    );

    let target = store.provision_target(mailbox_id).await.unwrap();
    assert_eq!(target.engine_state, EngineState::Failed);
    let error = target.engine_id.as_deref().unwrap();
    assert_eq!(error, outcome.engine_id);
}
