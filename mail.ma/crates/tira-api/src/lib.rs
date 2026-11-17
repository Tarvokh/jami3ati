//! Tira Mail HTTP API.
//!
//! Two surfaces over one storage layer:
//!
//! * a JSON read API under `/api/v1` for programmatic access, and
//! * a server-rendered UI (`/mail/...`) that a person uses in a browser.
//!
//! Both go through the same token resolution, RLS-scoped transaction, and audit
//! trail. The UI is not a second, weaker door.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tira_module::Registry;
use tira_store::PgStore;

use crate::limiter::LoginLimiter;

pub mod auth;
pub mod limiter;
pub mod pages;
pub mod proof;
pub mod read;
pub mod send;

/// One policy for every response: no external origins, no framing, no
/// referrer, no MIME sniffing. Inline styles and scripts are banned so the
/// policy can stay strict while the only scripts are the vendored HTMX build
/// and the tiny self-hosted navigation-accessibility enhancer.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
img-src 'self' data:; \
style-src 'self'; \
script-src 'self'; \
font-src 'self'; \
connect-src 'self'; \
object-src 'none'; \
base-uri 'none'; \
form-action 'self'; \
frame-ancestors 'none'";

/// Rejects state-changing form posts that a browser says came from another
/// origin. The session cookie is already `SameSite=Strict`, so this is a
/// second, explicit boundary: a cross-site form must not be able to log a
/// person into an attacker's account or change a password through a stolen
/// cookie. Non-browser clients that send no `Origin` are unaffected.
async fn same_origin_guard(request: Request, next: Next) -> Response {
    let state_changing = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if state_changing && !is_same_origin(request.headers()) {
        // A browser deserves the same designed, honest failure page as every
        // other error, not a bare string. Non-browser clients (curl, scripts)
        // keep the compact plain-text body they can grep.
        let wants_html = request
            .headers()
            .get(header::ACCEPT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"));
        if wants_html {
            return (
                StatusCode::FORBIDDEN,
                Html(tira_frontend::pages::cross_origin_rejected().into_string()),
            )
                .into_response();
        }
        let mut response = (
            StatusCode::FORBIDDEN,
            "cross-origin form submission rejected",
        )
            .into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        return response;
    }
    next.run(request).await
}

fn is_same_origin(headers: &axum::http::HeaderMap) -> bool {
    let site = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok());
    if site.is_some_and(|value| value.eq_ignore_ascii_case("cross-site")) {
        tracing::warn!("cross-origin form submission rejected: sec-fetch-site=cross-site");
        return false;
    }

    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        // Browsers send `Origin` on cross-origin requests; its absence is the
        // normal shape for same-origin form posts and for curl/tests.
        return true;
    };
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        tracing::warn!(%origin, "cross-origin form submission rejected: missing Host header");
        return false;
    };
    if origin.eq_ignore_ascii_case("null") {
        let same_origin_fetch = site.is_some_and(|value| value.eq_ignore_ascii_case("same-origin"));
        if same_origin_fetch && is_loopback_authority(host) {
            return true;
        }
        tracing::warn!(%origin, "cross-origin form submission rejected: opaque Origin without same-origin attestation");
        return false;
    }
    let Ok(uri) = origin.parse::<axum::http::Uri>() else {
        tracing::warn!(%origin, "cross-origin form submission rejected: unparseable Origin");
        return false;
    };
    let authority = uri.authority();

    let allowed = if let Some(authority) = authority {
        if authority.as_str().eq_ignore_ascii_case(host) {
            true
        } else {
            let same_origin_fetch =
                site.is_none_or(|value| value.eq_ignore_ascii_case("same-origin"));
            same_origin_fetch
                && is_loopback_authority(authority.as_str())
                && is_loopback_authority(host)
        }
    } else if site.is_none_or(|value| value.eq_ignore_ascii_case("same-origin"))
        && is_loopback_authority(host)
    {
        true
    } else {
        tracing::warn!(%origin, "cross-origin form submission rejected: Origin without authority");
        false
    };
    allowed
}
/// Whether an authority such as `localhost:8099`, `127.0.0.1:3000`, or
/// `[::1]:8099` points at the loopback interface. The port is ignored.
fn is_loopback_authority(authority: &str) -> bool {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host == "::1"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// Restricts the live-evidence endpoints to a verified loopback peer. The
/// payload describes the database role, whether RLS held, audit volume, and
/// DKIM key strength — reconnaissance an off-box attacker must not get. Only
/// the TCP peer (`ConnectInfo`) is trusted; `X-Forwarded-For` and friends are
/// deliberately ignored, so a spoofed header cannot grant access. With no
/// `ConnectInfo` (a router driven without connect info) the guard fails closed.
/// A non-loopback caller gets `404`, not `403`, so a remote scanner cannot even
/// confirm the route is mounted.
async fn loopback_only(request: Request, next: Next) -> Response {
    let is_loopback = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().is_loopback())
        .unwrap_or(false);
    if !is_loopback {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(request).await
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    // HSTS is emitted unconditionally on purpose: per RFC 6797 a browser MUST
    // ignore it over non-secure transport, so a local plain-HTTP demo is
    // unaffected, while production behind TLS pins HTTPS for a year and stops
    // SSL-strip before the (now Secure-by-default) session cookie can leak.
    // `preload` is deliberately omitted — that is a separate, explicit commit.
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), interest-cohort=()"),
    );
    response
}

#[derive(Clone)]
pub struct AppState {
    pub version: &'static str,
    pub started: Instant,
    pub modules: Registry,
    /// `None` means "run without persistence", which is only useful for tests
    /// and for `--help`-style local runs. Production must set it.
    pub store: Option<PgStore>,
    /// Address the SMTP listener bound to, reported by `/healthz`.
    pub smtp_addr: Option<SocketAddr>,
    /// Outbound submission relay (STARTTLS + credentials) from the operator
    /// environment. `None` means no relay is configured, and the send surface
    /// must refuse to submit instead of picking an unintended host.
    pub smtp_relay: Option<tira_smtp::client::SmtpRelay>,
    /// Where the live-evidence endpoint finds its artifacts, and how it
    /// describes the deployment. Overridable in tests and by `TIRA_*` vars.
    pub proof: proof::ProofConfig,
    /// In-process TTL cache for the loopback-only evidence endpoint, so a local
    /// poller cannot demand unbounded rebuilds. Shared through `Arc` like the
    /// limiter; see [`proof::ProofCache`].
    pub proof_cache: proof::ProofCache,
    /// Per-process failure ledger for password login and password changes.
    /// Shared through `Arc` because every cloned state must see the same
    /// counters, otherwise parallel requests could each get a fresh budget.
    pub login_limiter: Arc<LoginLimiter>,
    /// One-shot SMTP-handoff confirmations. Issued by the send handler and
    /// looked up by an unguessable id bound to the mailbox; in memory only, so
    /// a restart forgets them (and the UI never claims more than the relay's
    /// acceptance).
    pub sent_notices: Arc<send::SentNotices>,
}

impl AppState {
    /// Stateless state: no database, no SMTP. Used by the HTTP unit tests.
    pub fn new(version: &'static str) -> Self {
        Self {
            version,
            started: Instant::now(),
            modules: Registry::new(),
            store: None,
            smtp_addr: None,
            smtp_relay: None,
            proof: proof::ProofConfig::from_env(),
            proof_cache: proof::new_proof_cache(),
            login_limiter: Arc::new(LoginLimiter::standard()),
            sent_notices: Arc::new(send::SentNotices::new()),
        }
    }

    /// Full state: persistence plus the address mail is being accepted on.
    pub fn with_store(
        version: &'static str,
        store: PgStore,
        smtp_addr: Option<SocketAddr>,
    ) -> Self {
        Self {
            version,
            started: Instant::now(),
            modules: Registry::new(),
            store: Some(store),
            smtp_addr,
            smtp_relay: tira_smtp::client::SmtpRelay::from_env(),
            proof: proof::ProofConfig::from_env(),
            proof_cache: proof::new_proof_cache(),
            login_limiter: Arc::new(LoginLimiter::standard()),
            sent_notices: Arc::new(send::SentNotices::new()),
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }
}

#[derive(Debug, Serialize)]
pub struct HealthBody {
    /// `ok` when every dependency answered, `degraded` when one did not.
    pub status: &'static str,
    /// `ok`, `down`, or `disabled` (no database configured).
    pub database: &'static str,
}

#[derive(Debug, Serialize)]
pub struct VersionBody {
    pub product: &'static str,
    pub version: &'static str,
}

pub fn app(state: AppState) -> Router {
    // The evidence endpoints are loopback-only. A dedicated sub-router carries
    // the peer guard so it can neither leak onto the public routes nor be
    // accidentally omitted from these. `route_layer` runs after routing but
    // before the handler, so a non-loopback peer is refused with 404 before any
    // evidence is built. The alias serves the identical payload.
    let proof_routes = Router::new()
        .route(
            "/api/v1/control-plane-proof",
            get(proof::control_plane_proof),
        )
        .route("/api/v1/sovereignty-proof", get(proof::control_plane_proof))
        .route_layer(middleware::from_fn(loopback_only));

    Router::new()
        .route("/", get(pages::root))
        .route("/login", get(pages::login_form).post(pages::login_submit))
        .route("/logout", post(pages::logout))
        .route("/profile", get(pages::profile_page))
        .route("/profile/display-name", post(pages::profile_display_name))
        .route("/profile/password", post(pages::profile_password))
        .route(
            "/profile/sessions/revoke",
            post(pages::profile_revoke_session),
        )
        .route(
            "/profile/sessions/revoke-others",
            post(pages::profile_revoke_others),
        )
        .route("/assets/app.css", get(pages::asset_css))
        .route("/assets/fonts/{file}", get(pages::asset_font))
        .route("/assets/htmx.min.js", get(pages::asset_js))
        .route("/assets/nav-a11y.js", get(pages::asset_nav_a11y))
        .route("/healthz", get(healthz))
        .route("/api/v1/version", get(version))
        .route(
            "/api/v1/mailboxes/{mailbox_id}/messages",
            get(read::list_messages),
        )
        .route("/api/v1/messages/{message_id}/raw", get(read::raw_message))
        .route("/api/v1/outbound", post(send::send_api))
        .route("/mail/{mailbox_id}", get(pages::inbox_page))
        .route(
            "/mail/{mailbox_id}/message/{message_id}",
            get(pages::message_page),
        )
        .route(
            "/mail/{mailbox_id}/message/{message_id}/print",
            get(pages::message_print_page),
        )
        .route("/mail/{mailbox_id}/compose", get(pages::compose_page))
        .route("/mail/{mailbox_id}/send", post(send::send))
        .merge(proof_routes)
        // Guard first, headers last: the last `.layer` is the outermost one,
        // so even a 403 from the guard carries the CSP and framing policy.
        .layer(middleware::from_fn(same_origin_guard))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

/// Returns 503 when the database is configured but unreachable, so a load
/// balancer stops sending traffic to a process that cannot store mail.
///
/// The body is deliberately minimal: a liveness probe needs only whether the
/// process can serve (`status`) and whether its storage answered (`database`).
/// Uptime, module counts and the SMTP listener are internal topology details
/// that an unauthenticated caller has no reason to learn, so they are not
/// exposed here.
async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<HealthBody>) {
    let (database, code) = match &state.store {
        None => ("disabled", StatusCode::OK),
        Some(store) => match store.ping().await {
            Ok(()) => ("ok", StatusCode::OK),
            Err(err) => {
                tracing::warn!(%err, "database health check failed");
                ("down", StatusCode::SERVICE_UNAVAILABLE)
            }
        },
    };

    let body = HealthBody {
        status: if code == StatusCode::OK {
            "ok"
        } else {
            "degraded"
        },
        database,
    };

    (code, Json(body))
}

async fn version() -> Json<VersionBody> {
    Json(VersionBody {
        product: "Tira Mail",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::{is_loopback_authority, is_same_origin};
    use axum::http::{header, HeaderMap, HeaderValue};

    fn headers(host: &str, origin: Option<&str>, site: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        if let Some(site) = site {
            headers.insert("sec-fetch-site", HeaderValue::from_str(site).unwrap());
        }
        headers
    }

    #[test]
    fn exact_authority_is_allowed() {
        assert!(is_same_origin(&headers(
            "mail.tira.ma",
            Some("https://mail.tira.ma"),
            Some("same-origin")
        )));
        assert!(is_same_origin(&headers(
            "127.0.0.1:8099",
            Some("http://127.0.0.1:8099"),
            Some("same-origin")
        )));
    }

    #[test]
    fn missing_origin_is_allowed_for_non_browser_clients() {
        assert!(is_same_origin(&headers("127.0.0.1:8099", None, None)));
    }

    #[test]
    fn loopback_aliases_are_allowed_when_the_browser_says_same_origin() {
        // The exact shape that failed before: localhost in the address bar,
        // 127.0.0.1 as the Host seen by the server (preview/proxy rewrite).
        assert!(is_same_origin(&headers(
            "127.0.0.1:8099",
            Some("http://localhost:8099"),
            Some("same-origin")
        )));
        assert!(is_same_origin(&headers(
            "localhost:8099",
            Some("http://127.0.0.1:8099"),
            Some("same-origin")
        )));
        assert!(is_same_origin(&headers(
            "127.0.0.1:8099",
            Some("http://127.0.0.1:3000"),
            Some("same-origin")
        )));
        assert!(is_same_origin(&headers(
            "[::1]:8099",
            Some("http://localhost:3000"),
            None
        )));
    }

    #[test]
    fn loopback_alias_does_not_override_a_cross_site_fetch() {
        assert!(!is_same_origin(&headers(
            "127.0.0.1:8099",
            Some("http://localhost:3000"),
            Some("cross-site")
        )));
    }

    #[test]
    fn loopback_alias_requires_a_same_origin_fetch() {
        // Same-site but cross-origin (different port) from a local page must
        // not slip through; only a genuine same-origin post may.
        assert!(!is_same_origin(&headers(
            "127.0.0.1:8099",
            Some("http://localhost:3000"),
            Some("same-site")
        )));
    }

    #[test]
    fn public_cross_origin_is_rejected() {
        assert!(!is_same_origin(&headers(
            "mail.tira.ma",
            Some("https://evil.test"),
            Some("cross-site")
        )));
        assert!(!is_same_origin(&headers(
            "mail.tira.ma",
            Some("https://evil.test"),
            Some("same-origin")
        )));
    }

    #[test]
    fn malformed_or_missing_host_is_rejected() {
        let mut no_host = HeaderMap::new();
        no_host.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://mail.tira.ma"),
        );
        assert!(!is_same_origin(&no_host));

        let mut null_origin = HeaderMap::new();
        null_origin.insert(header::HOST, HeaderValue::from_static("mail.tira.ma"));
        null_origin.insert(header::ORIGIN, HeaderValue::from_static("null"));
        assert!(!is_same_origin(&null_origin));
    }

    #[test]
    fn null_origin_with_loopback_same_origin_fetch_is_allowed() {
        // Some embedded browsers (e.g. Brave/Chromium-in-app) attest a genuine
        // same-origin fetch while sending `Origin: null`. On loopback the Host
        // keeps this local/demo boundary, so the request must be allowed.
        assert!(is_same_origin(&headers(
            "127.0.0.1:8099",
            Some("null"),
            Some("same-origin")
        )));
        assert!(is_same_origin(&headers(
            "localhost:8099",
            Some("null"),
            Some("same-origin")
        )));
        // An opaque origin must still be rejected off loopback (public host).
        assert!(!is_same_origin(&headers(
            "mail.tira.ma",
            Some("null"),
            Some("same-origin")
        )));
    }

    #[test]
    fn loopback_authority_detection() {
        assert!(is_loopback_authority("localhost"));
        assert!(is_loopback_authority("localhost:8099"));
        assert!(is_loopback_authority("LOCALHOST.:8099"));
        assert!(is_loopback_authority("127.0.0.1:8099"));
        assert!(is_loopback_authority("127.1.2.3"));
        assert!(is_loopback_authority("[::1]:8099"));
        assert!(!is_loopback_authority("mail.tira.ma"));
        assert!(!is_loopback_authority("8.8.8.8:8099"));
        assert!(!is_loopback_authority("localhost.evil.test"));
    }
}
