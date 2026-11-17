//! HTTP read-path tests against a real PostgreSQL.
//!
//! These are the tests that would have caught the P1 gap: mail was stored but
//! nothing could read it. They also pin the P4 rule that a token — not a
//! guessed UUID — is what opens a mailbox.
//!
//! They need `TIRA_TEST_DATABASE_URL`; set `TIRA_REQUIRE_DATABASE=1` to make a
//! missing database a failure instead of a silent skip.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use chrono::Duration;
use tower::ServiceExt;
use uuid::Uuid;

use tira_api::{app, AppState};
use tira_smtp::message::parse;
use tira_smtp::server::Envelope;
use tira_store::{Mailbox, PgStore};

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero read-path coverage"
        );
    }

    url
}

fn temp_root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tira-api-read-{}", Uuid::new_v4()))
}

struct Fixture {
    store: PgStore,
    mailbox: Mailbox,
    /// Same tenant, different mailbox: used to prove scope, not just tenancy.
    other_mailbox: Mailbox,
    raw: Vec<u8>,
    message_id: Uuid,
    hostile_id: Uuid,
    /// Tenant-wide token for `mailbox`.
    token: String,
    /// Token restricted to `mailbox`; must not reach `other_mailbox`.
    scoped_token: String,
    /// A token from another tenant; must never see this fixture's mail.
    foreign_token: String,
}

async fn fixture(url: &str) -> Fixture {
    let store = PgStore::connect(url, &temp_root()).await.expect("connect");

    let tenant = store
        .create_tenant("Université Cadi Ayyad", Some("CNDP-TEST-0002"))
        .await
        .expect("tenant");
    let domain = store
        .create_domain(tenant.id, &format!("{}.test", Uuid::new_v4()))
        .await
        .expect("domain");
    let mailbox = store
        .create_mailbox(domain.id, "yassine", 5 * 1024 * 1024)
        .await
        .expect("mailbox");
    let other_mailbox = store
        .create_mailbox(domain.id, "aicha", 5 * 1024 * 1024)
        .await
        .expect("other mailbox");

    // A second tenant exists only to prove the boundary is enforced.
    let foreign_tenant = store
        .create_tenant("Université Mohammed V", Some("CNDP-TEST-0003"))
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
        .issue_token(tenant.id, None, "test-tenant-wide-r2", day)
        .await
        .expect("token")
        .secret;
    let scoped_token = store
        .issue_token(tenant.id, Some(mailbox.id), "test-scoped", day)
        .await
        .expect("scoped token")
        .secret;
    let foreign_token = store
        .issue_token(foreign_tenant.id, None, "test-foreign", day)
        .await
        .expect("foreign token")
        .secret;

    // Arabic subject, RFC 2047 encoded: proves the listing returns decoded
    // text rather than raw base64.
    let raw = b"From: Aicha <aicha@uca.ma>\r\n\
To: Yassine <yassine@tira.test>\r\n\
Subject: =?UTF-8?B?2KfZhNiz2YTYp9mFINi52YTZitmD2YU=?=\r\n\
Date: Tue, 03 Nov 2026 10:00:00 +0000\r\n\
Message-ID: <read-path@uca.ma>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Salam Yassine, hada test.\r\n"
        .to_vec();
    let parsed = parse(&raw).expect("parse");
    let envelope = Envelope {
        mail_from: "aicha@uca.ma".to_string(),
        rcpt_to: vec![mailbox.address.clone()],
    };
    let stored = store
        .append_message(&envelope, &parsed, &raw)
        .await
        .expect("append");
    assert_eq!(stored.len(), 1);

    // A hostile subject: the HTML view must escape it.
    let hostile_raw = b"From: Mallory <mallory@evil.test>\r\n\
To: Yassine <yassine@tira.test>\r\n\
Subject: <script>alert(1)</script>\r\n\
Date: Tue, 03 Nov 2026 11:00:00 +0000\r\n\
Message-ID: <hostile@evil.test>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
boom\r\n"
        .to_vec();
    let hostile_parsed = parse(&hostile_raw).expect("parse hostile");
    let hostile = store
        .append_message(&envelope, &hostile_parsed, &hostile_raw)
        .await
        .expect("append hostile");
    assert_eq!(hostile.len(), 1);

    Fixture {
        store,
        mailbox,
        other_mailbox,
        raw,
        message_id: stored[0].id,
        hostile_id: hostile[0].id,
        token,
        scoped_token,
        foreign_token,
    }
}

fn router(store: PgStore) -> axum::Router {
    app(AppState::with_store("0.1.0", store, None))
}

fn request(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn get_as(
    router: axum::Router,
    uri: &str,
    token: Option<&str>,
) -> (StatusCode, String, Vec<u8>) {
    let response = router.oneshot(request(uri, token)).await.expect("request");

    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("body")
        .to_vec();

    (status, content_type, bytes)
}

#[tokio::test]
async fn lists_messages_as_json() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let uri = format!("/api/v1/mailboxes/{}/messages", fx.mailbox.id);

    let (status, content_type, bytes) = get_as(router(fx.store), &uri, Some(&fx.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("application/json"));

    let body = String::from_utf8(bytes).expect("utf8");
    assert!(body.contains("\"total\":2"), "body was: {body}");
    assert!(body.contains(&fx.mailbox.address));
    assert!(body.contains("السلام عليكم"));
    // The blob key is an internal storage name; it must not leak.
    assert!(!body.contains("blob_key"), "body was: {body}");
}

#[tokio::test]
async fn raw_endpoint_returns_the_exact_accepted_bytes() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let uri = format!("/api/v1/messages/{}/raw", fx.message_id);

    let (status, content_type, bytes) = get_as(router(fx.store), &uri, Some(&fx.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("message/rfc822"));
    assert_eq!(bytes, fx.raw, "stored bytes must round-trip unchanged");
}

#[tokio::test]
async fn html_view_escapes_the_subject() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let uri = format!("/mail/{}", fx.mailbox.id);

    let (status, content_type, bytes) = get_as(router(fx.store), &uri, Some(&fx.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/html"));

    let html = String::from_utf8(bytes).expect("utf8");
    assert!(html.contains("dir=\"rtl\""));
    assert!(html.contains("السلام عليكم"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    // The two intentional `<script src>` tags are vendored, same-origin, and
    // carry no user data; what must never appear is the hostile subject turned
    // back into executable markup.
    assert!(
        !html.contains("<script>alert"),
        "untrusted subject rendered raw"
    );
    // And the message id is reachable, so the row is actually a link.
    assert!(html.contains(&fx.hostile_id.to_string()));
}

#[tokio::test]
async fn unknown_ids_are_404_and_malformed_ids_are_400() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;

    let missing = Uuid::new_v4();
    let (status, _, _) = get_as(
        router(fx.store.clone()),
        &format!("/api/v1/mailboxes/{missing}/messages"),
        Some(&fx.token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = get_as(
        router(fx.store.clone()),
        &format!("/api/v1/messages/{missing}/raw"),
        Some(&fx.token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = get_as(
        router(fx.store),
        "/api/v1/mailboxes/not-a-uuid/messages",
        Some(&fx.token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn without_a_database_the_read_path_is_503_not_404() {
    // No store configured: the process is alive but cannot answer. A client
    // must retry, not conclude the mailbox was deleted.
    let (status, _, _) = get_as(
        app(AppState::new("0.1.0")),
        &format!("/api/v1/mailboxes/{}/messages", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn a_missing_token_is_401_with_a_ww_authenticate_challenge() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let uri = format!("/api/v1/mailboxes/{}/messages", fx.mailbox.id);

    let response = router(fx.store)
        .oneshot(request(&uri, None))
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer realm=\"tira-mail\"")
    );
}

#[tokio::test]
async fn another_tenants_token_cannot_read_this_mail() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;

    let list = format!("/api/v1/mailboxes/{}/messages", fx.mailbox.id);
    let (status, _, _) = get_as(router(fx.store.clone()), &list, Some(&fx.foreign_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "list leaked across tenants");

    let raw = format!("/api/v1/messages/{}/raw", fx.message_id);
    let (status, _, _) = get_as(router(fx.store.clone()), &raw, Some(&fx.foreign_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "raw leaked across tenants");

    let page = format!("/mail/{}", fx.mailbox.id);
    let (status, _, _) = get_as(router(fx.store), &page, Some(&fx.foreign_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "page leaked across tenants");
}

#[tokio::test]
async fn a_mailbox_scoped_token_reaches_only_its_mailbox() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;

    let own = format!("/api/v1/mailboxes/{}/messages", fx.mailbox.id);
    let (status, _, _) = get_as(router(fx.store.clone()), &own, Some(&fx.scoped_token)).await;
    assert_eq!(status, StatusCode::OK);

    let sibling = format!("/api/v1/mailboxes/{}/messages", fx.other_mailbox.id);
    let (status, _, _) = get_as(router(fx.store), &sibling, Some(&fx.scoped_token)).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "scoped token escaped its mailbox"
    );
}

#[tokio::test]
async fn revoked_and_expired_tokens_are_rejected_the_same_way() {
    let Some(url) = database_url() else {
        return;
    };
    let fx = fixture(&url).await;
    let tenant = fx.mailbox.tenant_id;

    let revoked = fx
        .store
        .issue_token(tenant, None, "to-revoke", Some(Duration::days(1)))
        .await
        .expect("issue revoked");
    assert!(fx
        .store
        .revoke_token(revoked.record.id)
        .await
        .expect("revoke"));

    let expired = fx
        .store
        .issue_token(
            tenant,
            None,
            "already-expired",
            Some(Duration::seconds(-60)),
        )
        .await
        .expect("issue expired");

    let uri = format!("/api/v1/mailboxes/{}/messages", fx.mailbox.id);
    for token in [&revoked.secret, &expired.secret] {
        let (status, _, _) = get_as(router(fx.store.clone()), &uri, Some(token)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
