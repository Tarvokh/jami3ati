//! The CSRF guard must reject untrusted origins, but never silently and never
//! with an off-design body for a browser. These tests pin both halves: the
//! security decision (403) and the presentation (designed HTML for browsers,
//! compact plain text for scripts), plus the CSP that wraps even a rejection.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    tira_api::app(tira_api::AppState::new("0.1.0"))
}

async fn post_login(origin: Option<&str>, accept: Option<&str>) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/login")
        .header(header::HOST, "127.0.0.1:8080")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    if let Some(accept) = accept {
        builder = builder.header(header::ACCEPT, accept);
    }
    test_app()
        .oneshot(builder.body(Body::from("mode=token&token=x")).unwrap())
        .await
        .unwrap()
}

async fn body_of(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn opaque_origin_is_rejected_with_a_designed_page_for_browsers() {
    let response = post_login(Some("null"), Some("text/html,application/xhtml+xml")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("text/html; charset=utf-8")
    );
    // Even a rejection carries the framing / CSP policy (outermost layer).
    assert!(response.headers().get("content-security-policy").is_some());

    let body = body_of(response).await;
    assert!(body.contains("dir=\"rtl\""));
    assert!(body.contains("أصل الطلب غير موثوق"));
    assert!(body.contains("العودة إلى تسجيل الدخول"));
    // The page must not leak account or data state.
    assert!(!body.contains("yassine"));
}

#[tokio::test]
async fn opaque_origin_keeps_plain_text_for_non_browser_clients() {
    let response = post_login(Some("null"), None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("text/plain; charset=utf-8")
    );
    let body = body_of(response).await;
    assert_eq!(body, "cross-origin form submission rejected");
}

#[tokio::test]
async fn cross_site_origin_is_rejected_regardless_of_accept() {
    let response = post_login(Some("https://evil.test"), Some("text/html")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn same_origin_post_is_not_blocked_by_the_guard() {
    // The guard must let a genuine same-origin post through to the handler
    // (which then rejects the bad credential on its own, not with a 403).
    let response = post_login(Some("http://127.0.0.1:8080"), Some("text/html")).await;
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}
