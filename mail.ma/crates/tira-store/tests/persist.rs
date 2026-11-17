//! End-to-end persistence tests.
//!
//! These need a real PostgreSQL. They are skipped when
//! `TIRA_TEST_DATABASE_URL` is not set, so `cargo test` still works on a
//! machine without a database — but CI and the release checklist must set it.
//!
//! ```text
//! set TIRA_TEST_DATABASE_URL=postgres://postgres@127.0.0.1:55432/tira_test
//! cargo test -p tira-store --test persist
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

use tira_smtp::server::{Server, ServerConfig};
use tira_store::{Mailbox, PgStore};

/// Returns the test database URL, or `None` to skip.
///
/// A skipped test still prints green, so a CI job without a database would
/// report success while covering none of the storage layer. Setting
/// `TIRA_REQUIRE_DATABASE=1` turns a missing URL into a hard failure, which is
/// what CI and the release checklist use.
fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero storage coverage"
        );
    }

    url
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("tira-store-test-{}", Uuid::new_v4()))
}

/// Creates an isolated tenant/domain/mailbox so parallel tests never collide.
async fn fixture(store: &PgStore, quota_bytes: i64) -> (String, Mailbox) {
    let tenant = store
        .create_tenant("Université Cadi Ayyad", Some("CNDP-TEST-0001"))
        .await
        .expect("tenant");
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("domain");
    let mailbox = store
        .create_mailbox(domain.id, "yassine", quota_bytes)
        .await
        .expect("mailbox");
    (domain.name, mailbox)
}

async fn read_reply<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await.expect("read reply");
        assert!(read > 0, "connection closed while waiting for a reply");
        let line = line.trim_end().to_string();
        let more = line.as_bytes().get(3) == Some(&b'-');
        lines.push(line);
        if !more {
            return lines;
        }
    }
}

async fn start_server(store: PgStore) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = Server::new(ServerConfig::default(), store);
    let handle = tokio::spawn(async move {
        let _ = server.serve_on(listener).await;
    });
    (addr, handle)
}

/// Sends one message and returns the reply to the final dot.
async fn deliver(addr: SocketAddr, from: &str, to: &str, data: &str) -> (Vec<String>, Vec<String>) {
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));

    write_half.write_all(b"EHLO test.local\r\n").await.unwrap();
    let _ = read_reply(&mut reader).await;

    write_half
        .write_all(format!("MAIL FROM:<{from}>\r\n").as_bytes())
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    write_half
        .write_all(format!("RCPT TO:<{to}>\r\n").as_bytes())
        .await
        .unwrap();
    let rcpt = read_reply(&mut reader).await;

    if !rcpt[0].starts_with("250 ") {
        return (rcpt, Vec::new());
    }

    write_half.write_all(b"DATA\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("354 "));

    write_half.write_all(data.as_bytes()).await.unwrap();
    write_half.write_all(b".\r\n").await.unwrap();
    let final_reply = read_reply(&mut reader).await;

    write_half.write_all(b"QUIT\r\n").await.unwrap();
    let _ = read_reply(&mut reader).await;

    (rcpt, final_reply)
}

#[tokio::test]
async fn message_survives_a_restart() {
    let Some(url) = database_url() else {
        eprintln!("SKIP message_survives_a_restart: TIRA_TEST_DATABASE_URL not set");
        return;
    };

    let root = temp_root();
    let store = PgStore::connect(&url, &root).await.expect("connect");
    let (domain, mailbox) = fixture(&store, 5 * 1024 * 1024).await;
    let address = format!("yassine@{domain}");

    let (addr, server) = start_server(store.clone()).await;

    let data = "Subject: Persist test\r\n\r\nSalam, hada test.\r\n";
    let (rcpt, reply) = deliver(addr, "aicha@uca.ma", &address, data).await;
    assert!(rcpt[0].starts_with("250 "), "recipient: {rcpt:?}");
    assert!(reply[0].starts_with("250 "), "delivery: {reply:?}");

    // Simulate a restart: kill the server, drop the pool, reconnect.
    server.abort();
    let _ = server.await;
    drop(store);

    let reopened = PgStore::connect(&url, &root).await.expect("reconnect");
    let messages = reopened
        .list_mailbox(mailbox.id, 10, 0)
        .await
        .expect("list");

    assert_eq!(messages.len(), 1, "message must survive the restart");
    assert_eq!(messages[0].subject.as_deref(), Some("Persist test"));
    assert_eq!(messages[0].from_addr, "aicha@uca.ma");
    assert_eq!(messages[0].size_bytes, data.len() as i64);
    assert_eq!(
        reopened.raw_message(&messages[0].blob_key).expect("raw"),
        data.as_bytes(),
        "stored bytes must match what was accepted over SMTP"
    );

    // The mailbox accounting must reflect the stored message.
    let updated = reopened
        .resolve_recipient(&address)
        .await
        .expect("resolve")
        .expect("mailbox exists");
    assert_eq!(updated.used_bytes, data.len() as i64);
}

#[tokio::test]
async fn unknown_recipient_is_refused_at_rcpt() {
    let Some(url) = database_url() else {
        eprintln!("SKIP unknown_recipient_is_refused_at_rcpt: TIRA_TEST_DATABASE_URL not set");
        return;
    };

    let store = PgStore::connect(&url, temp_root()).await.expect("connect");
    let (domain, _mailbox) = fixture(&store, 1024 * 1024).await;
    let (addr, server) = start_server(store).await;

    let (rcpt, _) = deliver(
        addr,
        "aicha@uca.ma",
        &format!("nobody@{domain}"),
        "Subject: x\r\n\r\nbody\r\n",
    )
    .await;
    assert!(rcpt[0].starts_with("550 "), "expected 550, got {rcpt:?}");

    server.abort();
}

#[tokio::test]
async fn quota_is_enforced() {
    let Some(url) = database_url() else {
        eprintln!("SKIP quota_is_enforced: TIRA_TEST_DATABASE_URL not set");
        return;
    };

    let store = PgStore::connect(&url, temp_root()).await.expect("connect");
    let (domain, _mailbox) = fixture(&store, 64).await;
    let (addr, server) = start_server(store).await;

    let big = "Subject: too big\r\n\r\n".to_string() + &"x".repeat(200) + "\r\n";
    let (rcpt, reply) = deliver(addr, "aicha@uca.ma", &format!("yassine@{domain}"), &big).await;

    assert!(rcpt[0].starts_with("250 "), "recipient accepted: {rcpt:?}");
    assert!(reply[0].starts_with("552 "), "expected 552, got {reply:?}");

    server.abort();
}

#[tokio::test]
async fn purge_removes_only_dead_tokens_past_the_grace_window() {
    let Some(url) = database_url() else {
        eprintln!("SKIP purge_removes_only_dead_tokens_past_the_grace_window: TIRA_TEST_DATABASE_URL not set");
        return;
    };

    let store = PgStore::connect(&url, temp_root()).await.expect("connect");
    let tenant = store
        .create_tenant("Purge Test", None)
        .await
        .expect("tenant");

    // A live token, a long-dead expired token, and a just-expired token.
    let live = store
        .issue_token(tenant.id, None, "live", Some(chrono::Duration::days(1)))
        .await
        .expect("live token");
    let old_dead = store
        .issue_token(
            tenant.id,
            None,
            "old-dead",
            Some(chrono::Duration::days(-10)),
        )
        .await
        .expect("old dead token");
    let recent_dead = store
        .issue_token(
            tenant.id,
            None,
            "recent-dead",
            Some(chrono::Duration::seconds(-1)),
        )
        .await
        .expect("recent dead token");

    let ids = |tokens: Vec<tira_store::ApiToken>| -> Vec<uuid::Uuid> {
        tokens.into_iter().map(|t| t.id).collect()
    };

    // With a 7-day grace, only the token dead for 10 days is removed; the
    // just-expired one is still inside the retention window.
    store
        .purge_dead_tokens(chrono::Duration::days(7))
        .await
        .expect("purge grace 7");
    let present = ids(store.list_tokens(tenant.id).await.expect("list"));
    assert!(present.contains(&live.record.id), "live token was purged");
    assert!(
        present.contains(&recent_dead.record.id),
        "recent dead purged too early"
    );
    assert!(
        !present.contains(&old_dead.record.id),
        "old dead token survived purge"
    );

    // With zero grace the just-expired row goes too; the live one never does.
    store
        .purge_dead_tokens(chrono::Duration::zero())
        .await
        .expect("purge grace 0");
    let present = ids(store.list_tokens(tenant.id).await.expect("list"));
    assert!(present.contains(&live.record.id), "live token was purged");
    assert!(
        !present.contains(&recent_dead.record.id),
        "recent dead survived zero grace"
    );

    // The live secret still resolves; the dead ones never did.
    assert!(store
        .resolve_token(&live.secret)
        .await
        .expect("resolve")
        .is_some());
    assert!(store
        .resolve_token(&old_dead.secret)
        .await
        .expect("resolve")
        .is_none());
}

/// The login lookup must resolve an address to exactly one credential and miss
/// cleanly otherwise. This locks the sargable rewrite (`d.name = $1 AND
/// m.local_part = $2`, served by `domains_name_key` + `mailboxes_address_key`)
/// against a regression back to the concatenated, sequential-scanning
/// predicate. The store does not case-fold: the caller normalises to lowercase
/// before calling, and an un-normalised address must not silently match.
#[tokio::test]
async fn login_lookup_resolves_exactly_and_misses_cleanly() {
    let Some(url) = database_url() else {
        eprintln!(
            "SKIP login_lookup_resolves_exactly_and_misses_cleanly: TIRA_TEST_DATABASE_URL not set"
        );
        return;
    };

    let store = PgStore::connect(&url, temp_root()).await.expect("connect");
    let tenant = store
        .create_tenant("Login Lookup", None)
        .await
        .expect("tenant");
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("domain");
    let mailbox = store
        .create_mailbox(domain.id, "yassine", 1024 * 1024)
        .await
        .expect("mailbox");

    // The CHECK only requires the Argon2id PHC prefix; this is a lookup test,
    // not a password-verification test, so a representative PHC string suffices.
    let phc = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQxMjM0NTY$RdescudvJCsgt3ub+b+dWRWJTmaaJObG";
    store
        .set_password(mailbox.id, tenant.id, phc)
        .await
        .expect("set password");

    let address = format!("yassine@{}", domain.name);

    // The exact address resolves to this mailbox, tenant, address and hash.
    let found = store
        .credential_by_address(&address)
        .await
        .expect("lookup")
        .expect("credential exists");
    assert_eq!(found.mailbox_id, mailbox.id);
    assert_eq!(found.tenant_id, tenant.id);
    assert_eq!(found.address, address);
    assert_eq!(found.password_hash, phc);

    // Wrong local part on a real domain, and a real local part on a wrong
    // domain, both miss without error.
    assert!(store
        .credential_by_address(&format!("nobody@{}", domain.name))
        .await
        .expect("lookup")
        .is_none());
    assert!(store
        .credential_by_address("yassine@nonexistent.invalid")
        .await
        .expect("lookup")
        .is_none());

    // A malformed address (no '@') cannot match any row and resolves to None
    // rather than erroring or, worse, matching on a partial predicate.
    assert!(store
        .credential_by_address("no-at-sign")
        .await
        .expect("lookup")
        .is_none());
    assert!(store
        .credential_by_address("")
        .await
        .expect("lookup")
        .is_none());

    // The store does not case-fold; an un-normalised address must not match.
    assert!(store
        .credential_by_address(&address.to_uppercase())
        .await
        .expect("lookup")
        .is_none());
}
