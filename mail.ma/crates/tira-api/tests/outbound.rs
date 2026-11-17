//! Machine outbound submission (`POST /api/v1/outbound`) against real PostgreSQL.
//!
//! These tests pin the Jami3ati contract:
//!
//!   * acceptance is `202` and durable — the row exists in `outbound_queue`
//!     with the validated headers, not a claimed delivery;
//!   * a retried `idempotency_key` resolves to the same queue row;
//!   * a token can never speak for another tenant's mailbox;
//!   * header-shaped input is refused before it reaches the queue.
//!
//! They need `TIRA_TEST_DATABASE_URL`; `TIRA_REQUIRE_DATABASE=1` turns a missing
//! database into a failure instead of a silent skip.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use chrono::Duration;
use tower::ServiceExt;
use uuid::Uuid;

use tira_api::{app, AppState};
use tira_smtp::client::SmtpRelay;
use tira_store::{Mailbox, PgStore};

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|value| !value.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero outbound coverage"
        );
    }

    url
}

fn temp_root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tira-api-outbound-{}", Uuid::new_v4()))
}

struct Fixture {
    store: PgStore,
    mailbox: Mailbox,
    other_mailbox: Mailbox,
    /// Tenant-wide token for `mailbox`'s tenant.
    token: String,
    /// Token restricted to `mailbox`.
    scoped_token: String,
    /// A token from another tenant.
    foreign_token: String,
}

async fn fixture(url: &str) -> Fixture {
    let store = PgStore::connect(url, &temp_root()).await.expect("connect");

    let tenant = store
        .create_tenant("Université Cadi Ayyad", Some("CNDP-TEST-0010"))
        .await
        .expect("tenant");
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("domain");
    let mailbox = store
        .create_mailbox(domain.id, "no-reply", 5 * 1024 * 1024)
        .await
        .expect("mailbox");
    let other_mailbox = store
        .create_mailbox(domain.id, "other", 5 * 1024 * 1024)
        .await
        .expect("other mailbox");

    let foreign_tenant = store
        .create_tenant("Université Mohammed V", Some("CNDP-TEST-0011"))
        .await
        .expect("foreign tenant");
    let foreign_domain = store
        .create_domain(foreign_tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("foreign domain");
    store
        .create_mailbox(foreign_domain.id, "intruder", 1024 * 1024)
        .await
        .expect("foreign mailbox");

    let day = Some(Duration::days(1));
    let token = store
        .issue_token(tenant.id, None, "test-outbound-tenant", day)
        .await
        .expect("token")
        .secret;
    let scoped_token = store
        .issue_token(tenant.id, Some(mailbox.id), "test-outbound-scoped", day)
        .await
        .expect("scoped token")
        .secret;
    let foreign_token = store
        .issue_token(foreign_tenant.id, None, "test-outbound-foreign", day)
        .await
        .expect("foreign token")
        .secret;

    Fixture {
        store,
        mailbox,
        other_mailbox,
        token,
        scoped_token,
        foreign_token,
    }
}

fn router(store: PgStore, with_relay: bool) -> Router {
    let mut state = AppState::with_store(env!("CARGO_PKG_VERSION"), store, None);
    state.smtp_relay = if with_relay {
        Some(SmtpRelay {
            host: "127.0.0.1".to_string(),
            port: 2525,
            username: String::new(),
            password: String::new(),
            starttls: false,
        })
    } else {
        None
    };
    app(state)
}

fn payload(mailbox_id: Uuid, key: &str) -> serde_json::Value {
    serde_json::json!({
        "mailbox_id": mailbox_id.to_string(),
        "to": ["Etudiant@Example.MA"],
        "subject": "تحديث موعد الترشيح — ENSIAS",
        "text": "نص الرسالة",
        "idempotency_key": key,
        "correlation_id": "6f1c2a4e-1111-4222-8333-444455556666",
        "list_unsubscribe": "<https://jami3ati.ma/u/abc.def>",
        "reply_to": "no-reply@jami3ati.ma"
    })
}

async fn post_json(
    router: Router,
    token: Option<&str>,
    body: &serde_json::Value,
) -> (StatusCode, String) {
    let mut builder =
        Request::post("/api/v1/outbound").header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = builder
        .body(Body::from(body.to_string()))
        .expect("request body");
    let response = router.oneshot(request).await.expect("router");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (status, String::from_utf8(bytes.to_vec()).expect("utf8"))
}

/// Claims every due row and returns the one with `id`. The test database is
/// shared with other suites, so a fixed small batch could miss our row.
async fn claimed(store: &PgStore, id: &str) -> tira_store::OutboundQueueEntry {
    let entries = store.claim_due_outbound(2_000).await.expect("claim");
    entries
        .into_iter()
        .find(|entry| entry.id.to_string() == id)
        .expect("accepted row must be due in the queue")
}

#[tokio::test]
async fn accepts_durably_with_the_validated_headers() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;

    let (status, body) = post_json(
        router(fx.store.clone(), true),
        Some(&fx.token),
        &payload(fx.mailbox.id, "jami3ati:notif:0001"),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "body was: {body}");

    let json: serde_json::Value = serde_json::from_str(&body).expect("json body");
    assert_eq!(json["status"], "queued");
    assert_eq!(json["duplicate"], false);
    let id = json["id"].as_str().expect("queue id").to_string();

    let entry = claimed(&fx.store, &id).await;
    assert_eq!(entry.status, "pending");
    assert_eq!(entry.recipients, "etudiant@example.ma");
    assert_eq!(
        entry.correlation_id.as_deref(),
        Some("6f1c2a4e-1111-4222-8333-444455556666")
    );
    assert_eq!(
        entry.list_unsubscribe.as_deref(),
        Some("<https://jami3ati.ma/u/abc.def>")
    );
    assert_eq!(entry.reply_to.as_deref(), Some("no-reply@jami3ati.ma"));
    assert_eq!(
        entry.idempotency_key.as_deref(),
        Some("jami3ati:notif:0001")
    );
    // The claim is the first dispatch attempt; the row was pending before it.
    assert_eq!(entry.attempts, 1);
}

#[tokio::test]
async fn a_retried_key_returns_the_same_row_instead_of_a_second_send() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let key = "jami3ati:notif:0002";

    let (first_status, first_body) = post_json(
        router(fx.store.clone(), true),
        Some(&fx.token),
        &payload(fx.mailbox.id, key),
    )
    .await;
    assert_eq!(first_status, StatusCode::ACCEPTED);
    let first: serde_json::Value = serde_json::from_str(&first_body).expect("json");
    assert_eq!(first["duplicate"], false);

    let (second_status, second_body) = post_json(
        router(fx.store.clone(), true),
        Some(&fx.token),
        &payload(fx.mailbox.id, key),
    )
    .await;
    assert_eq!(second_status, StatusCode::ACCEPTED);
    let second: serde_json::Value = serde_json::from_str(&second_body).expect("json");
    assert_eq!(second["duplicate"], true);
    assert_eq!(
        second["id"], first["id"],
        "the retry must resolve to one row"
    );
}

#[tokio::test]
async fn a_missing_token_is_401() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let (status, _) = post_json(
        router(fx.store.clone(), true),
        None,
        &payload(fx.mailbox.id, "jami3ati:notif:0003"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_token_cannot_speak_for_another_tenants_mailbox() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let (status, body) = post_json(
        router(fx.store.clone(), true),
        Some(&fx.foreign_token),
        &payload(fx.mailbox.id, "jami3ati:notif:0004"),
    )
    .await;
    // Same answer as "does not exist": the mailbox id space must not be
    // enumerable across tenants.
    assert_eq!(status, StatusCode::NOT_FOUND, "body was: {body}");
}

#[tokio::test]
async fn a_scoped_token_cannot_name_another_mailbox() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let (status, _) = post_json(
        router(fx.store.clone(), true),
        Some(&fx.scoped_token),
        &payload(fx.other_mailbox.id, "jami3ati:notif:0005"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_tenant_wide_token_must_name_a_mailbox() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let mut body = payload(fx.mailbox.id, "jami3ati:notif:0006");
    body.as_object_mut().expect("object").remove("mailbox_id");

    let (status, response) =
        post_json(router(fx.store.clone(), true), Some(&fx.token), &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body was: {response}");
}

#[tokio::test]
async fn header_shaped_input_is_refused_before_the_queue() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let mut body = payload(fx.mailbox.id, "jami3ati:notif:0007");
    body["subject"] = serde_json::json!("ok\r\nBcc: evil@example.com");

    let (status, response) =
        post_json(router(fx.store.clone(), true), Some(&fx.token), &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body was: {response}");
}

#[tokio::test]
async fn a_missing_relay_refuses_to_accept() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let (status, body) = post_json(
        router(fx.store.clone(), false),
        Some(&fx.token),
        &payload(fx.mailbox.id, "jami3ati:notif:0008"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body was: {body}");
}
