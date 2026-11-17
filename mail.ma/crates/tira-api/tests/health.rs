use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    tira_api::app(tira_api::AppState::new("0.1.0"))
}

#[tokio::test]
async fn healthz_reports_ok_r2() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"status\":\"ok\""));
    // Without a configured database the process is healthy but honest about it.
    assert!(body.contains("\"database\":\"disabled\""));
    // The probe is minimal: internal topology (uptime, module count, SMTP
    // listener) must not leak to an unauthenticated caller.
    assert!(
        !body.contains("uptime_secs"),
        "uptime must not leak: {body}"
    );
    assert!(
        !body.contains("modules"),
        "module count must not leak: {body}"
    );
    assert!(
        !body.contains("smtp"),
        "SMTP listener must not leak: {body}"
    );
    // Exactly the two fields a liveness probe needs, nothing more.
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 2);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["database"], "disabled");
}

#[tokio::test]
async fn version_endpoint_identifies_the_product() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Tira Mail"));
}

#[tokio::test]
async fn the_inbox_screen_is_rendered_by_the_server() {
    let response = test_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/html; charset=utf-8")
    );

    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("dir=\"rtl\""));
    assert!(body.contains("Tira Mail"));
    assert!(body.contains("<svg "));
    assert!(!body.contains("http://api.iconify.design"));
}

#[tokio::test]
async fn unknown_route_is_404() {
    let response = test_app()
        .oneshot(Request::builder().uri("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn every_response_carries_the_security_headers() {
    let response = test_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let headers = response.headers();
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());

    // HSTS: emitted unconditionally; browsers ignore it over plain HTTP, so the
    // local demo is unaffected while production behind TLS pins HTTPS.
    assert_eq!(
        get("strict-transport-security"),
        Some("max-age=31536000; includeSubDomains")
    );
    assert_eq!(get("x-frame-options"), Some("DENY"));
    assert_eq!(get("x-content-type-options"), Some("nosniff"));
    assert_eq!(get("referrer-policy"), Some("no-referrer"));
    assert!(get("content-security-policy")
        .is_some_and(|csp| csp.contains("default-src 'self'") && !csp.contains("unsafe-inline")));

    // The rendered page is a mailbox/login surface: never retained by a cache.
    assert_eq!(get("cache-control"), Some("no-store"));
    assert_eq!(get("vary"), Some("Cookie"));
}
