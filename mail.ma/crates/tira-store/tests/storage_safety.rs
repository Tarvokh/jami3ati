//! Storage-safety tests for Unit B (H-03, M-10).
//!
//! Three properties that the schema and the blob layer alone cannot prove:
//!
//! 1. **Quota refusal writes nothing.** A delivery to a full mailbox must be
//!    rejected *before* any bytes hit the disk, so a sender cannot exhaust the
//!    volume by hammering a full account (H-03 pre-flight).
//! 2. **The orphan sweep is precise.** It removes unreferenced blobs past the
//!    grace window, keeps every referenced blob, and spares a recent orphan
//!    that an in-flight transaction might still commit against (H-03 GC).
//! 3. **Fan-out delivery is audited per tenant.** One envelope addressed to
//!    mailboxes in two tenants leaves a `message.delivered` row in *each*
//!    tenant's trail, not just the first (M-10, CNDP 09-08).
//!
//! These need a real PostgreSQL; `TIRA_REQUIRE_DATABASE=1` turns a missing
//! database into a failure instead of a silent skip.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use tira_smtp::message::parse;
use tira_smtp::server::Envelope;
use tira_store::blob::{BlobStore, FsBlobStore};
use tira_store::{PgStore, StoreError};

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero storage-safety coverage"
        );
    }

    url
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("tira-store-safety-{}", Uuid::new_v4()))
}

/// A tenant with one domain and one mailbox, returning the full address.
async fn fixture(store: &PgStore, label: &str, quota: i64) -> (Uuid, String) {
    let tenant = store.create_tenant(label, None).await.expect("tenant");
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("domain");
    let mailbox = store
        .create_mailbox(domain.id, "owner", quota)
        .await
        .expect("mailbox");
    (tenant.id, mailbox.address)
}

fn envelope(from: &str, to: Vec<String>) -> Envelope {
    Envelope {
        mail_from: from.to_string(),
        rcpt_to: to,
    }
}

/// Backdates a file's mtime so the grace window can be tested without waiting.
fn age(path: &std::path::Path, by: Duration) {
    let when = SystemTime::now() - by;
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open to age")
        .set_modified(when)
        .expect("set modified");
}

/// Counts content files under the blob root, ignoring the `tmp/` scratch dir.
fn content_files(root: &std::path::Path) -> usize {
    fn walk(dir: &std::path::Path, out: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "tmp") {
                    continue;
                }
                walk(&path, out);
            } else {
                *out += 1;
            }
        }
    }
    let mut count = 0;
    walk(root, &mut count);
    count
}

#[tokio::test]
async fn quota_refusal_writes_no_blob_to_disk() {
    let Some(url) = database_url() else {
        return;
    };
    let root = temp_root();
    let store = PgStore::connect(&url, &root).await.expect("connect");
    // A mailbox with room for only 16 bytes.
    let (_tenant, address) = fixture(&store, "Safety Quota", 16).await;

    let raw = b"Subject: too big\r\n\r\nThis body is far larger than sixteen bytes.\r\n";
    let parsed = parse(raw).expect("parse");
    let env = envelope("sender@external.test", vec![address.clone()]);

    let err = store
        .append_message(&env, &parsed, raw)
        .await
        .expect_err("over-quota delivery must fail");
    assert!(
        matches!(err, StoreError::QuotaExceeded(ref a) if a == &address),
        "expected QuotaExceeded, got {err:?}"
    );

    // The pre-flight check must have refused before `blobs.put`: the volume
    // holds no content file, so repeated refusals cannot exhaust the disk.
    assert_eq!(
        content_files(&root),
        0,
        "a quota refusal must not leave an orphan blob on disk"
    );
}

#[tokio::test]
async fn orphan_sweep_removes_unreferenced_keeps_referenced_and_spares_recent() {
    let Some(url) = database_url() else {
        return;
    };
    let root = temp_root();
    let store = PgStore::connect(&url, &root).await.expect("connect");
    let (_tenant, address) = fixture(&store, "Safety Sweep", 5 * 1024 * 1024).await;

    // A real delivery: its blob is referenced by a committed message.
    let raw = b"Subject: kept\r\n\r\nThese bytes are referenced and must survive.\r\n";
    let parsed = parse(raw).expect("parse");
    let env = envelope("sender@external.test", vec![address]);
    let stored = store
        .append_message(&env, &parsed, raw)
        .await
        .expect("deliver");
    let referenced_key = stored[0].blob_key.clone();

    // Two orphans written straight to the same root, with unique content so no
    // other test's message can reference them. One is aged past the grace
    // window; one is recent (an in-flight delivery might still commit it).
    let blobs = FsBlobStore::new(&root).expect("blob store");
    let old_orphan = blobs
        .put(format!("orphan-old-{}", Uuid::new_v4()).as_bytes())
        .expect("old orphan");
    let recent_orphan = blobs
        .put(format!("orphan-recent-{}", Uuid::new_v4()).as_bytes())
        .expect("recent orphan");
    age(&root.join(&old_orphan.key), Duration::from_secs(7200));

    // Grace = 1h: the 2h-old orphan is collected; the recent one is spared;
    // the referenced blob is never touched.
    let removed = store
        .purge_unreferenced_blobs(chrono::Duration::hours(1))
        .await
        .expect("sweep grace 1h");
    assert_eq!(removed, 1, "only the aged orphan should be collected");
    assert!(
        !blobs.exists(&old_orphan.key),
        "aged orphan survived the sweep"
    );
    assert!(
        blobs.exists(&recent_orphan.key),
        "recent orphan swept too early"
    );
    assert!(blobs.exists(&referenced_key), "referenced blob was swept");
    assert_eq!(
        store.raw_message(&referenced_key).expect("raw"),
        raw,
        "the live message's bytes must remain readable"
    );

    // Age the recent orphan and sweep with zero grace: it goes now, the
    // referenced blob still stays.
    age(&root.join(&recent_orphan.key), Duration::from_secs(1800));
    let removed = store
        .purge_unreferenced_blobs(chrono::Duration::zero())
        .await
        .expect("sweep grace 0");
    assert_eq!(removed, 1, "the aged recent orphan should now be collected");
    assert!(!blobs.exists(&recent_orphan.key));
    assert!(
        blobs.exists(&referenced_key),
        "referenced blob swept at zero grace"
    );

    // A third sweep is a no-op: nothing unreferenced remains.
    let removed = store
        .purge_unreferenced_blobs(chrono::Duration::zero())
        .await
        .expect("sweep again");
    assert_eq!(removed, 0, "sweep must be idempotent");
}

#[tokio::test]
async fn fan_out_delivery_audits_every_recipient_tenant() {
    let Some(url) = database_url() else {
        return;
    };
    let root = temp_root();
    let store = PgStore::connect(&url, &root).await.expect("connect");
    let (tenant_a, addr_a) = fixture(&store, "Safety Audit A", 1024 * 1024).await;
    let (tenant_b, addr_b) = fixture(&store, "Safety Audit B", 1024 * 1024).await;
    assert_ne!(tenant_a, tenant_b);

    let raw = b"Subject: fan-out\r\n\r\nOne message, two tenants.\r\n";
    let parsed = parse(raw).expect("parse");
    let env = envelope("sender@external.test", vec![addr_a, addr_b]);
    let stored = store
        .append_message(&env, &parsed, raw)
        .await
        .expect("deliver to both");
    assert_eq!(stored.len(), 2, "one copy per recipient");
    let blob_key = stored[0].blob_key.clone();
    assert_eq!(stored[1].blob_key, blob_key, "deduplicated to one blob");

    // Each tenant's trail must carry its own delivery row for this blob.
    for tenant in [tenant_a, tenant_b] {
        let trail = store.recent_audit(tenant, 50).await.expect("audit");
        let deliveries: Vec<_> = trail
            .iter()
            .filter(|e| e.action == "message.delivered" && e.object.as_deref() == Some(&blob_key))
            .collect();
        assert_eq!(
            deliveries.len(),
            1,
            "tenant {tenant} must have exactly one delivery row for the shared blob"
        );
        assert_eq!(deliveries[0].tenant_id, Some(tenant));
        assert_eq!(deliveries[0].actor, "sender@external.test");
    }
}

/// Low (lock ordering): a fan-out delivery must take its `FOR UPDATE` row
/// locks in one global order — ascending mailbox id — so two concurrent
/// deliveries to overlapping mailboxes listed in opposite envelope order
/// cannot deadlock. The observable contract is that the returned copies come
/// back sorted by mailbox id regardless of the order the envelope listed.
#[tokio::test]
async fn fan_out_locks_mailboxes_in_deterministic_order() {
    let Some(url) = database_url() else {
        return;
    };
    let root = temp_root();
    let store = PgStore::connect(&url, &root).await.expect("connect");

    let tenant = store
        .create_tenant("Safety Lock Order", None)
        .await
        .expect("tenant");
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("domain");
    let mut mailboxes = Vec::new();
    for local in ["alpha", "bravo", "charlie"] {
        mailboxes.push(
            store
                .create_mailbox(domain.id, local, 1024 * 1024)
                .await
                .expect("mailbox"),
        );
    }

    // List recipients in descending id order — the exact arrangement that,
    // left unsorted, would lock rows in a different order than an ascending
    // peer delivery and form a deadlock cycle.
    let mut ascending = mailboxes.clone();
    ascending.sort_by_key(|m| m.id);
    let envelope_order: Vec<String> = ascending.iter().rev().map(|m| m.address.clone()).collect();

    let raw = b"Subject: lock order\r\n\r\nOne envelope, three mailboxes.\r\n";
    let parsed = parse(raw).expect("parse");
    let env = envelope("sender@external.test", envelope_order);
    let stored = store
        .append_message(&env, &parsed, raw)
        .await
        .expect("deliver to three");
    assert_eq!(stored.len(), 3, "one copy per recipient");

    let returned: Vec<Uuid> = stored.iter().map(|m| m.mailbox_id).collect();
    let expected: Vec<Uuid> = ascending.iter().map(|m| m.id).collect();
    assert_eq!(
        returned, expected,
        "copies must come back in ascending mailbox-id order regardless of envelope order"
    );
}
