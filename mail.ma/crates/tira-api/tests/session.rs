//! Session-security tests against a real PostgreSQL.
//!
//! These pin the fix for the logout gap: clearing the cookie used to leave the
//! `api_tokens` row alive for its full 7-day TTL, so a secret captured before
//! logout kept working. Logout must revoke the token, not just the browser copy.
//!
//! They need `TIRA_TEST_DATABASE_URL`; set `TIRA_REQUIRE_DATABASE=1` to make a
//! missing database a failure instead of a silent skip.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use chrono::Duration;
use tower::ServiceExt;
use uuid::Uuid;

use tira_api::{app, AppState};
use tira_store::PgStore;

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero session coverage"
        );
    }

    url
}

fn temp_root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tira-api-session-{}", Uuid::new_v4()))
}

/// A mailbox-scoped web session token, plus the store and app that issued it.
async fn setup(url: &str) -> (PgStore, String, axum::Router) {
    let store = PgStore::connect(url, &temp_root()).await.expect("connect");
    let tenant = store
        .create_tenant("Session Test Tenant", Some("CNDP-TEST-SESSION"))
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
    let secret = store
        .issue_token(
            tenant.id,
            Some(mailbox.id),
            "session:web",
            Some(Duration::days(7)),
        )
        .await
        .expect("session token")
        .secret;

    let router = app(AppState::with_store("0.1.0", store.clone(), None));
    (store, secret, router)
}

#[tokio::test]
async fn logout_revokes_the_session_token_not_just_the_cookie() {
    let Some(url) = database_url() else {
        eprintln!("skipping: TIRA_TEST_DATABASE_URL not set");
        return;
    };
    let (store, secret, router) = setup(&url).await;

    // The token works before logout.
    assert!(
        store
            .resolve_token(&secret)
            .await
            .expect("resolve")
            .is_some(),
        "session must be live before logout"
    );

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout")
                .header(header::COOKIE, format!("tira_token={secret}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/login");
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .expect("clearing cookie");
    assert!(cookie.contains("Max-Age=0"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("HttpOnly"));

    // The row is dead: a captured secret can no longer open the mailbox.
    assert!(
        store
            .resolve_token(&secret)
            .await
            .expect("resolve")
            .is_none(),
        "logout must revoke the api_tokens row, not only clear the browser cookie"
    );
}

#[tokio::test]
async fn logout_without_a_session_still_clears_the_cookie() {
    let Some(url) = database_url() else {
        eprintln!("skipping: TIRA_TEST_DATABASE_URL not set");
        return;
    };
    let (_store, _secret, router) = setup(&url).await;

    // No cookie at all: idempotent, never loops through the form, never panics.
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/login");
    assert!(response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|c| c.contains("Max-Age=0")));
}

#[tokio::test]
async fn logout_with_an_already_dead_token_is_idempotent() {
    let Some(url) = database_url() else {
        eprintln!("skipping: TIRA_TEST_DATABASE_URL not set");
        return;
    };
    let (store, secret, router) = setup(&url).await;

    // Revoke out-of-band, then log out with the dead secret: still a clean
    // redirect, no error, no second audit pretending a live session ended.
    let token = store
        .resolve_token(&secret)
        .await
        .expect("resolve")
        .expect("live token");
    store.revoke_token(token.id).await.expect("revoke");

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout")
                .header(header::COOKIE, format!("tira_token={secret}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/login");
}
