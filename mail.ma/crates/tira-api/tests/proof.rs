//! HTTP tests for the live-evidence endpoint.
//!
//! The important property is not "the JSON has the right keys" but "the JSON
//! changes when the evidence changes": a missing artifact is an outage, a weak
//! key is a failed check, and a `p=none` policy is never reported as `reject`.
//!
//! Database tests follow the same convention as `read.rs`: they skip when
//! `TIRA_TEST_DATABASE_URL` is absent, and `TIRA_REQUIRE_DATABASE=1` turns
//! that skip into a failure.

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use tira_api::proof::ProofConfig;
use tira_api::{app, AppState};
use tira_store::PgStore;

fn database_url() -> Option<String> {
    let url = std::env::var("TIRA_TEST_DATABASE_URL")
        .ok()
        .filter(|value| !value.is_empty());

    if url.is_none() && std::env::var("TIRA_REQUIRE_DATABASE").as_deref() == Ok("1") {
        panic!(
            "TIRA_REQUIRE_DATABASE=1 but TIRA_TEST_DATABASE_URL is not set; \
             refusing to report a green run with zero proof-endpoint coverage"
        );
    }

    url
}

fn workspace_artifacts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("deploy")
        .join("dns")
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tira-api-proof-{label}-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn proof_config(dir: PathBuf) -> ProofConfig {
    ProofConfig {
        dir,
        domain: "tira.test".to_string(),
        environment: "test".to_string(),
        database_host_class: "loopback".to_string(),
    }
}

/// Connects, runs migrations, and creates one tenant so the audit write path
/// has something legitimate to probe with.
async fn connected_state(url: &str, dir: PathBuf) -> AppState {
    let store = PgStore::connect(url, &temp_dir("blobs"))
        .await
        .expect("connect");
    store
        .create_tenant("Proof endpoint fixture", None)
        .await
        .expect("tenant");
    let mut state = AppState::with_store("test", store, None);
    state.proof = proof_config(dir);
    state
}

/// A verified loopback TCP peer. The evidence endpoints are loopback-only, so
/// every happy-path test must present one; `into_make_service_with_connect_info`
/// injects this in production, and `call_from` injects it here.
fn loopback_peer() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 9))
}

async fn call(state: AppState, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    call_from(state, uri, Some(loopback_peer())).await
}

/// Drives the router with an explicit peer. `None` models a request that reached
/// the router without connect info, which the loopback guard must fail closed.
async fn call_from(
    state: AppState,
    uri: &str,
    peer: Option<SocketAddr>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut request = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    if let Some(addr) = peer {
        request.extensions_mut().insert(ConnectInfo(addr));
    }
    let response = app(state).oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec();
    (status, headers, body)
}

fn as_json(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("JSON body")
}

fn check<'a>(proof: &'a Value, id: &str) -> &'a Value {
    proof["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .find(|check| check["id"] == id)
        .unwrap_or_else(|| panic!("missing check {id}: {proof}"))
}

fn no_store(headers: &axum::http::HeaderMap) {
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "proof responses must never be cached"
    );
}

#[tokio::test]
async fn real_artifacts_produce_a_passing_proof() {
    let Some(url) = database_url() else {
        return;
    };
    let state = connected_state(&url, workspace_artifacts()).await;

    // The hackathon alias must serve the exact same live contract.
    let (alias_status, alias_headers, alias_body) =
        call(state.clone(), "/api/v1/sovereignty-proof").await;
    assert_eq!(alias_status, StatusCode::OK);
    no_store(&alias_headers);
    let alias = as_json(&alias_body);
    assert_eq!(alias["status"], "pass", "{alias:#}");

    let (status, headers, body) = call(state, "/api/v1/control-plane-proof").await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    no_store(&headers);

    let proof = as_json(&body);
    assert_eq!(proof["status"], "pass", "{proof:#}");

    // Tenant isolation is a live probe, not a migration quote.
    assert_eq!(
        proof["control_plane"]["tenant_isolation"]["method"],
        "live_probe"
    );
    assert_eq!(
        proof["control_plane"]["tenant_isolation"]["role"],
        "tira_app"
    );
    assert_eq!(
        proof["control_plane"]["tenant_isolation"]["bypassrls"],
        false
    );
    assert_eq!(
        proof["control_plane"]["tenant_isolation"]["random_tenant_rows"],
        0
    );
    assert_eq!(
        proof["control_plane"]["crown_jewels"]["deny_all_verified"],
        true
    );
    assert_eq!(proof["control_plane"]["audit"]["insert_probe"], "verified");

    // The effective DMARC policy comes from the zone: p=reject.
    assert_eq!(proof["mail_authentication"]["dmarc"]["policy"], "reject");
    assert_eq!(proof["mail_authentication"]["dmarc"]["stage"], "final");
    assert_eq!(
        proof["mail_authentication"]["artifact"]["name"],
        "tira.test.dns.json"
    );
    assert_eq!(proof["mail_authentication"]["spf"], "v=spf1 mx -all");
    // DKIM selectors rotate: old and new keys coexist during a rollover, so
    // this array legitimately grows. Assert a floor plus the structural
    // guarantees (both algorithms present, every published key verified)
    // instead of a frozen count that would break on every rotation.
    let dkim = proof["mail_authentication"]["dkim"]
        .as_array()
        .expect("dkim");
    assert!(
        dkim.len() >= 2,
        "expected at least one RSA and one Ed25519 selector: {dkim:#?}"
    );
    assert!(
        dkim.iter().any(|key| key["algo"] == "rsa"),
        "no rsa selector published: {dkim:#?}"
    );
    assert!(
        dkim.iter().any(|key| key["algo"] == "ed25519"),
        "no ed25519 selector published: {dkim:#?}"
    );
    for key in dkim {
        assert_eq!(
            key["verified"], true,
            "published DKIM key did not verify: {key:#}"
        );
    }
    assert_eq!(proof["no_external_assets"]["verified"], true);
    assert_eq!(proof["no_external_assets"]["external_references"], 0);

    // What the endpoint cannot prove is stated instead of implied.
    let not_proven = proof["scope"]["does_not_prove"]
        .as_array()
        .expect("does_not_prove");
    assert!(not_proven.iter().any(|item| item == "hosting_jurisdiction"));
    assert!(not_proven
        .iter()
        .any(|item| item == "public_dns_propagation"));

    // No absolute path, database port, or zoneFile field may leak.
    let text = String::from_utf8_lossy(&body);
    assert!(!text.contains("zoneFile"), "{text}");
    assert!(!text.contains("C:\\\\"), "absolute path leaked: {text}");
    assert!(!text.contains("55432"), "database port leaked: {text}");
}

#[tokio::test]
async fn missing_artifact_is_an_outage_not_a_pass() {
    let Some(url) = database_url() else {
        return;
    };
    let empty = temp_dir("missing");
    let state = connected_state(&url, empty).await;

    let (status, headers, body) = call(state, "/api/v1/control-plane-proof").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    no_store(&headers);
    let body = as_json(&body);
    assert_eq!(body["status"], "unavailable");
}

#[tokio::test]
async fn no_database_is_an_outage() {
    // No database is needed to prove this: the endpoint refuses to invent
    // evidence it cannot verify.
    let mut state = AppState::new("test");
    state.proof = proof_config(workspace_artifacts());

    let (status, headers, body) = call(state, "/api/v1/control-plane-proof").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    no_store(&headers);
    assert_eq!(as_json(&body)["status"], "unavailable");
}

#[tokio::test]
async fn weak_rsa_key_fails_the_check_without_a_503() {
    let Some(url) = database_url() else {
        return;
    };

    // A real 1024-bit SubjectPublicKeyInfo, generated for this test only.
    const WEAK_RSA: &str = "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQCAMFV6n8TpDjNYfaLH7BE2W4Clyu8UOV6DqM3yFzxhhqvQ9Ro/ZImu0/gdQmeMsdb7IEVqj7TZ/iNIbZK33AEmS3CVut8EKU5zmL3iByxRdpvA5QovVHmew+gNMld8ocbrEDVaf6TJ7hM4XYKnzPEWO2CFqs/0GT5jiK3S9xxBZgIDAQAB";

    let real_zone =
        std::fs::read_to_string(workspace_artifacts().join("tira.test.zone")).expect("real zone");

    // Weaken EVERY RSA selector, not just one. DKIM rotation publishes old and
    // new selectors side by side, and `dkim.rsa` passes when *any* RSA selector
    // reaches 2048 bits; the failure path is only exercised when none does.
    // Both the single-line and the multi-line (parenthesised) record formats
    // are handled, and RSA is detected from the record's `k=rsa` tag rather
    // than a hardcoded selector name, so the test survives future rotations.
    let mut weak_zone = real_zone;
    let mut cursor = 0;
    let mut weakened = 0;
    while let Some(rel) = weak_zone[cursor..].find("._domainkey.") {
        let hit = cursor + rel;
        let record_start = weak_zone[..hit].rfind('\n').map(|p| p + 1).unwrap_or(0);
        // Resolve the record bounds and algorithm into owned values only, so
        // no borrow of `weak_zone` is held across the mutation below.
        let (record_end, is_rsa, selector) = {
            let tail = &weak_zone[record_start..];
            let after_txt = tail.find("IN TXT").expect("DKIM TXT record") + "IN TXT".len();
            let (end, content) = if tail[after_txt..].trim_start().starts_with('(') {
                let close = tail.find("\n)").expect("closing parenthesis") + 2;
                (record_start + close, &tail[..close])
            } else {
                let nl = tail.find('\n').expect("record newline") + 1;
                (record_start + nl, &tail[..nl])
            };
            (
                end,
                content.contains("k=rsa"),
                tail[..hit - record_start].to_string(),
            )
        };
        if is_rsa {
            let weak_record = format!(
                "{selector}._domainkey.tira.test. IN TXT \"v=DKIM1; k=rsa; h=sha256; p={WEAK_RSA}\"\n"
            );
            weak_zone = format!(
                "{}{}{}",
                &weak_zone[..record_start],
                weak_record,
                &weak_zone[record_end..]
            );
            cursor = record_start + weak_record.len();
            weakened += 1;
        } else {
            cursor = record_end;
        }
    }
    assert!(
        weakened >= 1,
        "expected at least one RSA selector to weaken"
    );

    let dir = temp_dir("weak-rsa");
    std::fs::write(dir.join("tira.test.zone"), weak_zone).expect("write zone");
    let json =
        std::fs::read(workspace_artifacts().join("tira.test.dns.json")).expect("real artifact");
    std::fs::write(dir.join("tira.test.dns.json"), json).expect("write artifact");

    let state = connected_state(&url, dir).await;
    let (status, _, body) = call(state, "/api/v1/control-plane-proof").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a weak key is evidence of failure, not an outage"
    );

    let proof = as_json(&body);
    assert_eq!(proof["status"], "fail", "{proof:#}");
    assert_eq!(check(&proof, "dkim.rsa")["status"], "fail");
    assert_eq!(check(&proof, "artifact.active_keys")["status"], "fail");
}

#[tokio::test]
async fn dmarc_none_is_reported_honestly_as_monitoring() {
    let Some(url) = database_url() else {
        return;
    };

    let real_zone =
        std::fs::read_to_string(workspace_artifacts().join("tira.test.zone")).expect("real zone");
    assert!(real_zone.contains("p=reject"));
    let staged_zone = real_zone.replace("p=reject", "p=none");

    let dir = temp_dir("dmarc-none");
    std::fs::write(dir.join("tira.test.zone"), staged_zone).expect("write zone");
    let json =
        std::fs::read(workspace_artifacts().join("tira.test.dns.json")).expect("real artifact");
    std::fs::write(dir.join("tira.test.dns.json"), json).expect("write artifact");

    let state = connected_state(&url, dir).await;
    let (status, _, body) = call(state, "/api/v1/control-plane-proof").await;
    assert_eq!(status, StatusCode::OK);

    let proof = as_json(&body);
    assert_eq!(
        proof["mail_authentication"]["dmarc"]["policy"], "none",
        "the endpoint must read the zone, not the artifact's dmarcPolicy field"
    );
    assert_eq!(proof["mail_authentication"]["dmarc"]["stage"], "monitoring");
    assert_eq!(check(&proof, "dmarc")["status"], "warn");
    // A monitoring policy is honest, not a failure: the overall status stays
    // green because every hard guarantee still holds.
    assert_eq!(proof["status"], "pass", "{proof:#}");
}

#[tokio::test]
async fn malformed_artifact_json_is_an_outage() {
    let Some(url) = database_url() else {
        return;
    };

    let dir = temp_dir("malformed");
    std::fs::write(
        dir.join("tira.test.dns.json"),
        "{ this is not json".as_bytes(),
    )
    .expect("write artifact");
    let zone = std::fs::read(workspace_artifacts().join("tira.test.zone")).expect("real zone");
    std::fs::write(dir.join("tira.test.zone"), zone).expect("write zone");

    let state = connected_state(&url, dir).await;
    let (status, headers, body) = call(state, "/api/v1/control-plane-proof").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    no_store(&headers);
    let proof = as_json(&body);
    assert_eq!(proof["status"], "unavailable");
    assert!(!String::from_utf8_lossy(&body).contains("C:\\\\"));
}

#[tokio::test]
async fn a_non_loopback_peer_cannot_see_the_evidence_endpoint() {
    // No database is needed: the guard runs before the handler, so a remote
    // peer is refused even on a stateless process. 203.0.113.0/24 is TEST-NET-3
    // — unmistakably off-box.
    let state = AppState::new("test");
    let remote = SocketAddr::from(([203, 0, 113, 7], 9));

    for uri in ["/api/v1/control-plane-proof", "/api/v1/sovereignty-proof"] {
        let (status, _headers, body) = call_from(state.clone(), uri, Some(remote)).await;
        // 404, not 403: a scanner must not even confirm the route is mounted.
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        let text = String::from_utf8_lossy(&body);
        assert!(!text.contains("control_plane"), "evidence leaked: {text}");
        assert!(!text.contains("checks"), "evidence leaked: {text}");
    }
}

#[tokio::test]
async fn a_request_without_connect_info_fails_closed() {
    // If the router is ever driven without connect info, the peer is unknown
    // and the guard must refuse rather than assume loopback.
    let state = AppState::new("test");
    let (status, _headers, _body) = call_from(state, "/api/v1/control-plane-proof", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_forwarded_header_cannot_spoof_loopback() {
    // Only the TCP peer counts. A non-loopback client that claims to be
    // 127.0.0.1 / ::1 through proxy headers is still refused.
    let state = AppState::new("test");
    let mut request = Request::builder()
        .uri("/api/v1/control-plane-proof")
        .header("x-forwarded-for", "127.0.0.1")
        .header("x-real-ip", "::1")
        .header(header::HOST, "localhost")
        .body(Body::empty())
        .expect("request");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([203, 0, 113, 7], 9))));

    let response = app(state).oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_evidence_payload_is_cached_within_the_ttl() {
    let Some(url) = database_url() else {
        return;
    };
    let state = connected_state(&url, workspace_artifacts()).await;

    // `generated_at` is `Utc::now()` at build time. Two independent builds would
    // differ; an identical value across two calls proves the second was served
    // from the in-process cache rather than rebuilt.
    let (first_status, first_headers, first_body) =
        call(state.clone(), "/api/v1/control-plane-proof").await;
    assert_eq!(first_status, StatusCode::OK);
    no_store(&first_headers);
    let first = as_json(&first_body);

    let (second_status, second_headers, second_body) =
        call(state, "/api/v1/control-plane-proof").await;
    assert_eq!(second_status, StatusCode::OK);
    // The cache is in-process only; the HTTP no-store contract is unchanged.
    no_store(&second_headers);
    let second = as_json(&second_body);

    assert_eq!(
        first["generated_at"], second["generated_at"],
        "a cache hit must reuse the original build timestamp"
    );
    assert_eq!(first["status"], "pass");
    assert_eq!(second["status"], "pass");
}
