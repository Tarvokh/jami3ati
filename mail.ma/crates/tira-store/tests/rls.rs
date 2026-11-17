//! Direct proof that PostgreSQL — not our SQL — enforces tenant isolation.
//!
//! The HTTP tests in `tira-api/tests/read.rs` prove the read path returns 404
//! across tenants. That is necessary but not sufficient: they exercise our
//! handlers, which also carry hand-written `tenant_id = $2` predicates. If the
//! policies in migration `0004_rls.sql` were wrong, those predicates could
//! mask the mistake.
//!
//! These tests speak to the database directly as `tira_app` — the role the
//! read path drops to — and run predicates with **no** tenant filter. What
//! comes back is what the policy allows. Three scenarios:
//!
//! 1. **Zero trust.** No `tira.tenant_id` set -> nothing is visible. The
//!    failure mode of an unset GUC must be "see nothing", never "see
//!    everything".
//! 2. **Horizontal isolation.** With tenant A set, exactly A's rows are
//!    visible; B's rows are invisible even without a predicate.
//! 3. **Audit anti-spoofing.** `tira_app` may append audit rows for its own
//!    tenant, but `WITH CHECK` rejects a row that claims another tenant.
//! 4. **Crown-jewel tables are deny-all.** `api_tokens` (token hashes) and
//!    `message_recipients` have RLS enabled, no policy, and no `GRANT` to
//!    `tira_app`: even a valid tenant context cannot read or write them.
//!    Token resolution goes through `tira_resolve_token` only.
//!
//! They need `TIRA_TEST_DATABASE_URL`; `TIRA_REQUIRE_DATABASE=1` turns a
//! missing database into a failure instead of a silent skip.

use tokio_postgres::error::SqlState;
use tokio_postgres::{Client, NoTls};
use uuid::Uuid;

use tira_store::PgStore;

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero RLS coverage"
        );
    }

    url
}

fn temp_root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tira-store-rls-{}", Uuid::new_v4()))
}

/// A tenant with one domain and one mailbox. Unique per call so parallel tests
/// never collide.
async fn tenant_with_mailbox(store: &PgStore, label: &str) -> (Uuid, Uuid) {
    let tenant = store.create_tenant(label, None).await.expect("tenant");
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("domain");
    let mailbox = store
        .create_mailbox(domain.id, "owner", 1024 * 1024)
        .await
        .expect("mailbox");
    (tenant.id, mailbox.id)
}

/// Opens a raw connection. The store is connected first so migrations — and
/// therefore `tira_app` and the policies — exist before the role is used.
async fn raw_client(url: &str) -> Client {
    let (client, connection) = tokio_postgres::connect(url, NoTls)
        .await
        .expect("raw connect");
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            eprintln!("raw PostgreSQL connection ended: {err}");
        }
    });
    client
}

/// Drops the session to the unprivileged role and, when given, sets the
/// transaction-local tenant GUC. Both are exactly what the read path does.
async fn scoped_tx(client: &mut Client, tenant: Option<Uuid>) -> tokio_postgres::Transaction<'_> {
    let tx = client.transaction().await.expect("begin");
    if let Some(tenant) = tenant {
        tx.execute(
            "SELECT set_config('tira.tenant_id', $1, true)",
            &[&tenant.to_string()],
        )
        .await
        .expect("set tenant");
    }
    tx.batch_execute("SET LOCAL ROLE tira_app")
        .await
        .expect("set role");
    tx
}

#[tokio::test]
async fn zero_trust_without_a_tenant_guc_sees_nothing() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, &temp_root()).await.expect("connect");
    let (tenant_a, _) = tenant_with_mailbox(&store, "RLS Zero Trust A").await;
    let (tenant_b, _) = tenant_with_mailbox(&store, "RLS Zero Trust B").await;
    assert_ne!(tenant_a, tenant_b);

    let mut client = raw_client(&url).await;

    // Sanity: the role really is the unprivileged one. If a future migration
    // ever grants BYPASSRLS, this assertion fails before the others can
    // produce a misleading pass.
    let tx = scoped_tx(&mut client, None).await;
    let bypasses: bool = tx
        .query_one(
            "SELECT rolbypassrls FROM pg_roles WHERE rolname = current_user",
            &[],
        )
        .await
        .expect("role flags")
        .get(0);
    assert!(!bypasses, "tira_app must never have BYPASSRLS");

    // No GUC set: every policy's USING expression is NULL, so no row matches.
    let mailboxes: i64 = tx
        .query_one("SELECT count(*) FROM mailboxes", &[])
        .await
        .expect("count mailboxes")
        .get(0);
    let tenants: i64 = tx
        .query_one("SELECT count(*) FROM tenants", &[])
        .await
        .expect("count tenants")
        .get(0);

    assert_eq!(mailboxes, 0, "unset GUC exposed mailboxes");
    assert_eq!(tenants, 0, "unset GUC exposed tenants");

    // The audit trail is not even readable by this role: it has INSERT and no
    // SELECT, so an operator query cannot be answered, let alone one tenant
    // reading another's trail. 42501 is the proof.
    let audit_err = tx
        .query("SELECT count(*) FROM audit_log", &[])
        .await
        .expect_err("tira_app must not be able to read audit_log");
    assert_eq!(
        audit_err.code(),
        Some(&SqlState::INSUFFICIENT_PRIVILEGE),
        "expected 42501 reading audit_log, got: {audit_err}"
    );

    // The failed statement aborted the transaction; a fresh one proves the
    // empty-string case.
    tx.rollback().await.expect("rollback");

    // An empty string is not the same as unset; `nullif(..., '')` in
    // `tira_current_tenant()` must still resolve to NULL rather than error or
    // match anything.
    let tx = scoped_tx(&mut client, None).await;
    tx.execute("SELECT set_config('tira.tenant_id', '', true)", &[])
        .await
        .expect("set empty tenant");
    let mailboxes: i64 = tx
        .query_one("SELECT count(*) FROM mailboxes", &[])
        .await
        .expect("count mailboxes after empty GUC")
        .get(0);
    assert_eq!(mailboxes, 0, "empty-string GUC exposed mailboxes");

    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn tenant_a_sees_only_its_own_rows_without_a_predicate() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, &temp_root()).await.expect("connect");
    let (tenant_a, mailbox_a) = tenant_with_mailbox(&store, "RLS Isolation A").await;
    let (tenant_b, mailbox_b) = tenant_with_mailbox(&store, "RLS Isolation B").await;
    assert_ne!(tenant_a, tenant_b);

    let mut client = raw_client(&url).await;
    let tx = scoped_tx(&mut client, Some(tenant_a)).await;

    // No `WHERE tenant_id = ...` on purpose: the policy is the only filter.
    let ids: Vec<Uuid> = tx
        .query("SELECT id FROM mailboxes", &[])
        .await
        .expect("select mailboxes")
        .iter()
        .map(|row| row.get(0))
        .collect();

    assert_eq!(ids, vec![mailbox_a], "tenant A saw the wrong mailboxes");
    assert!(!ids.contains(&mailbox_b), "tenant B's mailbox leaked");

    let tenants: Vec<Uuid> = tx
        .query("SELECT id FROM tenants", &[])
        .await
        .expect("select tenants")
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(tenants, vec![tenant_a], "tenant A saw another tenant");

    tx.rollback().await.expect("rollback");
}

/// Runs one statement as `tira_app` for a valid tenant and returns the error.
/// A denied statement aborts its transaction, so every attempt gets a fresh
/// one; otherwise the second attempt would fail with "transaction is aborted"
/// and hide the real cause.
async fn denied_as_app(client: &mut Client, tenant: Uuid, sql: &str) -> tokio_postgres::Error {
    let tx = scoped_tx(client, Some(tenant)).await;
    let err = tx
        .execute(sql, &[])
        .await
        .expect_err("tira_app must not reach a crown-jewel table");
    tx.rollback().await.expect("rollback after denial");
    err
}

#[tokio::test]
async fn crown_jewel_tables_are_deny_all_for_the_app_role() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, &temp_root()).await.expect("connect");
    let (tenant_a, _) = tenant_with_mailbox(&store, "RLS Crown Jewels A").await;

    let mut client = raw_client(&url).await;

    // Positive control as the owner: both tables exist and have RLS enabled.
    // Without this, a mistyped table name would also "fail" — but with 42P01
    // (undefined_table) instead of the 42501 we assert below.
    let owner_tx = client.transaction().await.expect("begin owner");
    let flags: Vec<(String, bool)> = owner_tx
        .query(
            "SELECT relname, relrowsecurity FROM pg_class
              WHERE relname IN ('api_tokens', 'message_recipients')
              ORDER BY relname",
            &[],
        )
        .await
        .expect("rls flags")
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    assert_eq!(
        flags,
        vec![
            ("api_tokens".to_string(), true),
            ("message_recipients".to_string(), true),
        ],
        "both crown-jewel tables must exist with RLS enabled"
    );
    let _: i64 = owner_tx
        .query_one("SELECT count(*) FROM api_tokens", &[])
        .await
        .expect("the owner role must still be able to read api_tokens")
        .get(0);
    owner_tx.rollback().await.expect("rollback owner");

    // Every command on both tables, as `tira_app`, with a valid tenant GUC.
    // `tira_app` has neither SELECT nor INSERT/UPDATE/DELETE grants there, so
    // the tenant context is irrelevant: the privilege check rejects first.
    let id = Uuid::new_v4();
    let statements = [
        "SELECT count(*) FROM api_tokens".to_string(),
        format!(
            "INSERT INTO api_tokens (id, tenant_id, label, token_hash)
             VALUES ('{id}', '{tenant_a}', 'forged', '\\x00'::bytea)"
        ),
        "UPDATE api_tokens SET label = 'forged'".to_string(),
        "DELETE FROM api_tokens".to_string(),
        "SELECT count(*) FROM message_recipients".to_string(),
        format!(
            "INSERT INTO message_recipients (message_id, address)
             VALUES ('{id}', 'forged@evil.test')"
        ),
        "UPDATE message_recipients SET address = 'forged@evil.test'".to_string(),
        "DELETE FROM message_recipients".to_string(),
    ];

    for sql in statements {
        let err = denied_as_app(&mut client, tenant_a, &sql).await;
        assert_eq!(
            err.code(),
            Some(&SqlState::INSUFFICIENT_PRIVILEGE),
            "expected 42501 for `{sql}`, got: {err}"
        );
    }

    // Second lock: even if a future migration accidentally grants SELECT, RLS
    // is enabled with no policy, so the rows stay invisible. `GRANT` is
    // transactional in PostgreSQL; this is rolled back and cannot affect
    // another test or a live process.
    for table in ["api_tokens", "message_recipients"] {
        let tx = client.transaction().await.expect("begin grant probe");
        tx.batch_execute(&format!("GRANT SELECT ON {table} TO tira_app"))
            .await
            .expect("temporary grant");
        tx.execute(
            "SELECT set_config('tira.tenant_id', $1, true)",
            &[&tenant_a.to_string()],
        )
        .await
        .expect("set tenant");
        tx.batch_execute("SET LOCAL ROLE tira_app")
            .await
            .expect("set role");
        let visible: i64 = tx
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .await
            .expect("select under a temporary grant")
            .get(0);
        assert_eq!(
            visible, 0,
            "RLS with no policy must hide {table} even when SELECT is granted"
        );
        tx.rollback().await.expect("rollback grant probe");
    }
}

#[tokio::test]
async fn audit_rows_cannot_be_forged_for_another_tenant() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, &temp_root()).await.expect("connect");
    let (tenant_a, _) = tenant_with_mailbox(&store, "RLS Audit A").await;
    let (tenant_b, _) = tenant_with_mailbox(&store, "RLS Audit B").await;
    assert_ne!(tenant_a, tenant_b);

    let mut client = raw_client(&url).await;
    let tx = scoped_tx(&mut client, Some(tenant_a)).await;

    // Positive control: appending for *its own* tenant is allowed. Without
    // this, a missing INSERT grant would masquerade as a working policy.
    tx.execute(
        "INSERT INTO audit_log (tenant_id, actor, action, result)
         VALUES ($1, 'token:rls-test', 'rls.probe', 'ok')",
        &[&tenant_a],
    )
    .await
    .expect("tira_app must be able to append its own audit row");

    // The forgery: a row that claims tenant B while the GUC says tenant A.
    // `WITH CHECK` must reject it with SQLSTATE 42501, before the row exists.
    let err = tx
        .execute(
            "INSERT INTO audit_log (tenant_id, actor, action, result)
             VALUES ($1, 'fake', 'fake', 'ok')",
            &[&tenant_b],
        )
        .await
        .expect_err("tira_app forged an audit row for another tenant");

    assert_eq!(
        err.code(),
        Some(&SqlState::INSUFFICIENT_PRIVILEGE),
        "expected 42501 from WITH CHECK, got: {err}"
    );

    // The failed statement aborts the transaction; roll back explicitly so the
    // connection is returned clean.
    tx.rollback().await.expect("rollback");
}

/// Delivers one real message into a mailbox, so `messages` has a row whose
/// tenant the policy can filter. Uses the owner-role store path.
async fn deliver_one(store: &PgStore, address: &str) {
    let raw = b"Subject: rls probe\r\n\r\nbody\r\n";
    let parsed = tira_smtp::message::parse(raw).expect("parse");
    let env = tira_smtp::server::Envelope {
        mail_from: "sender@external.test".to_string(),
        rcpt_to: vec![address.to_string()],
    };
    store
        .append_message(&env, &parsed, raw)
        .await
        .expect("deliver");
}

#[tokio::test]
async fn domains_and_messages_are_isolated_without_a_predicate() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, &temp_root()).await.expect("connect");
    let (tenant_a, mailbox_a) = tenant_with_mailbox(&store, "RLS DomMsg A").await;
    let (tenant_b, mailbox_b) = tenant_with_mailbox(&store, "RLS DomMsg B").await;
    assert_ne!(tenant_a, tenant_b);

    let addr_a = store
        .get_mailbox(mailbox_a)
        .await
        .expect("get a")
        .expect("a")
        .address;
    let addr_b = store
        .get_mailbox(mailbox_b)
        .await
        .expect("get b")
        .expect("b")
        .address;
    deliver_one(&store, &addr_a).await;
    deliver_one(&store, &addr_b).await;

    // Domain ids, read as the owner (the app role test below must not lean on
    // a hand-written predicate to know what to expect).
    let owner = raw_client(&url).await;
    let domain_a: Uuid = owner
        .query_one("SELECT id FROM domains WHERE tenant_id = $1", &[&tenant_a])
        .await
        .expect("domain a")
        .get(0);
    let domain_b: Uuid = owner
        .query_one("SELECT id FROM domains WHERE tenant_id = $1", &[&tenant_b])
        .await
        .expect("domain b")
        .get(0);

    let mut client = raw_client(&url).await;
    let tx = scoped_tx(&mut client, Some(tenant_a)).await;

    // No WHERE tenant_id: the domains policy is the only filter.
    let domains: Vec<Uuid> = tx
        .query("SELECT id FROM domains", &[])
        .await
        .expect("select domains")
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(domains, vec![domain_a], "tenant A saw the wrong domains");
    assert!(!domains.contains(&domain_b), "tenant B's domain leaked");

    // Same for messages: only A's copy is visible.
    let mailboxes: Vec<Uuid> = tx
        .query("SELECT mailbox_id FROM messages", &[])
        .await
        .expect("select messages")
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(
        mailboxes,
        vec![mailbox_a],
        "tenant A saw the wrong messages"
    );
    assert!(!mailboxes.contains(&mailbox_b), "tenant B's message leaked");

    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn dkim_private_key_column_is_denied_to_the_app_role() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, &temp_root()).await.expect("connect");
    let (tenant_a, _) = tenant_with_mailbox(&store, "RLS DKIM A").await;

    let mut client = raw_client(&url).await;

    // Positive control: the columns the read path actually uses are granted.
    let tx = scoped_tx(&mut client, Some(tenant_a)).await;
    let names: Vec<String> = tx
        .query("SELECT name FROM domains", &[])
        .await
        .expect("select granted column")
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(names.len(), 1, "tenant A's domain name must be readable");
    tx.rollback().await.expect("rollback");

    // The signing key column is not granted: column-level privilege denies it
    // before RLS even matters. 42501 is the proof.
    let err = denied_as_app(
        &mut client,
        tenant_a,
        "SELECT dkim_private_enc FROM domains",
    )
    .await;
    assert_eq!(
        err.code(),
        Some(&SqlState::INSUFFICIENT_PRIVILEGE),
        "expected 42501 reading dkim_private_enc, got: {err}"
    );

    // `SELECT *` now fails too, because it includes the ungranted column. This
    // is the point: a future careless query cannot exfiltrate the key.
    let err = denied_as_app(&mut client, tenant_a, "SELECT * FROM domains").await;
    assert_eq!(
        err.code(),
        Some(&SqlState::INSUFFICIENT_PRIVILEGE),
        "expected 42501 for SELECT * on domains, got: {err}"
    );
}

#[tokio::test]
async fn composite_fk_rejects_a_message_claiming_the_wrong_tenant() {
    let Some(url) = database_url() else {
        return;
    };
    let store = PgStore::connect(&url, &temp_root()).await.expect("connect");
    let (tenant_a, mailbox_a) = tenant_with_mailbox(&store, "RLS FK A").await;
    let (tenant_b, _) = tenant_with_mailbox(&store, "RLS FK B").await;
    assert_ne!(tenant_a, tenant_b);

    let client = raw_client(&url).await;

    // mailbox_a belongs to tenant A. Claiming tenant B must violate the
    // composite foreign key (mailbox_id, tenant_id) -> mailboxes(id, tenant_id).
    let id = Uuid::new_v4();
    let err = client
        .execute(
            "INSERT INTO messages
                 (id, tenant_id, mailbox_id, from_addr, size_bytes, blob_key, sha256)
             VALUES ($1, $2, $3, 'x@y.test', 10, 'aa/bb/probe', 'deadbeef')",
            &[&id, &tenant_b, &mailbox_a],
        )
        .await
        .expect_err("composite FK must reject a cross-tenant message");
    assert_eq!(
        err.code(),
        Some(&SqlState::FOREIGN_KEY_VIOLATION),
        "expected 23503 from the composite FK, got: {err}"
    );

    // Positive control: the consistent pair is accepted, then cleaned up.
    let ok_id = Uuid::new_v4();
    client
        .execute(
            "INSERT INTO messages
                 (id, tenant_id, mailbox_id, from_addr, size_bytes, blob_key, sha256)
             VALUES ($1, $2, $3, 'x@y.test', 10, 'aa/bb/probe', 'deadbeef')",
            &[&ok_id, &tenant_a, &mailbox_a],
        )
        .await
        .expect("consistent tenant/mailbox pair must be accepted");
    client
        .execute("DELETE FROM messages WHERE id = $1", &[&ok_id])
        .await
        .expect("cleanup");
}
