//! Bearer-token authentication for the read API.
//!
//! Before this module, any caller who knew a mailbox UUID could read it. The
//! token is the smallest thing that fixes that without building a session
//! system: it carries a tenant, an optional mailbox restriction, and nothing
//! else. Authorization is intentionally *not* role-based yet — a token either
//! reaches its tenant's mail or it does not.
//!
//! Failure shapes are deliberate:
//!   * no/invalid token       -> 401, with `WWW-Authenticate: Bearer`
//!   * valid token, wrong box -> 404, identical to "does not exist"
//!   * no database configured -> 503 (the process cannot verify anything)
//!
//! The 404 choice prevents using this endpoint to enumerate other tenants'
//! mailbox and message ids.

use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::AppState;

/// The verified identity behind one request.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub token_id: Uuid,
    pub tenant_id: Uuid,
    /// `None` = every mailbox in the tenant; `Some(id)` = only that mailbox.
    pub mailbox_id: Option<Uuid>,
}

/// Extractor: add `auth: Auth` to a handler to require a valid token.
#[derive(Debug, Clone)]
pub struct Auth(pub AuthContext);

#[derive(Debug)]
pub enum AuthError {
    /// No database: we cannot verify a token, so we must not serve mail.
    NoStore,
    Missing,
    /// Resolving the token failed for an infrastructure reason, not because
    /// the token was bad. 503 tells the client to retry.
    Unavailable(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::NoStore => (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({ "error": "storage unavailable" })),
            )
                .into_response(),
            AuthError::Missing => unauthorized("missing bearer token"),
            AuthError::Unavailable(err) => {
                tracing::warn!(%err, "token lookup failed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(serde_json::json!({ "error": "storage unavailable" })),
                )
                    .into_response()
            }
        }
    }
}

fn unauthorized(message: &'static str) -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": message })),
    )
        .into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Bearer realm=\"tira-mail\""),
    );
    response
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let store = state.store.as_ref().ok_or(AuthError::NoStore)?;
        let secret = extract_secret(parts).ok_or(AuthError::Missing)?;

        match store.resolve_token(secret).await {
            Ok(Some(token)) => Ok(Auth(AuthContext {
                token_id: token.id,
                tenant_id: token.tenant_id,
                mailbox_id: token.mailbox_id,
            })),
            // Unknown, revoked, and expired are the same answer on purpose.
            Ok(None) => Err(AuthError::Missing),
            Err(err) => Err(AuthError::Unavailable(err.to_string())),
        }
    }
}

/// Page-level authentication.
///
/// Same verification as [`Auth`], but the failure shape is meant for a browser:
/// a missing/invalid session redirects to `/login` instead of returning a JSON
/// 401 that a person cannot act on. A storage failure is *not* a login problem,
/// so it renders the 503 page and never loops the user through the form.
#[derive(Debug, Clone)]
pub struct PageAuth(pub AuthContext);

impl FromRequestParts<AppState> for PageAuth {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match Auth::from_request_parts(parts, state).await {
            Ok(Auth(context)) => Ok(Self(context)),
            Err(AuthError::Missing) => Err(axum::response::Redirect::to("/login").into_response()),
            Err(AuthError::NoStore) => Err(crate::pages::service_unavailable_response(
                crate::locale::resolve(&parts.headers, None),
                "no database configured",
            )),
            Err(AuthError::Unavailable(err)) => {
                tracing::warn!(%err, "token lookup failed");
                Err(crate::pages::service_unavailable_response(
                    crate::locale::resolve(&parts.headers, None),
                    "storage unavailable",
                ))
            }
        }
    }
}

/// Accepts `Authorization: Bearer <secret>` first, then the `tira_token`
/// cookie for the server-rendered pages. Query-string tokens are rejected:
/// they end up in logs, Referer headers, and screenshots.
fn extract_secret(parts: &Parts) -> Option<&str> {
    secret_from_headers(&parts.headers)
}

/// Shared with the `/` handler, which decides between "show the login form"
/// and "send this session to its inbox".
pub(crate) fn secret_from_headers(headers: &axum::http::HeaderMap) -> Option<&str> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        if let Some(secret) = tira_store::token::parse_bearer(value) {
            return Some(secret);
        }
    }

    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|pair| {
                let (name, value) = pair.trim().split_once('=')?;
                (name == "tira_token" && !value.is_empty()).then_some(value)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn parts(headers: &[(&str, &str)]) -> Parts {
        let mut builder = Request::builder();
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(()).unwrap().into_parts().0
    }

    #[test]
    fn bearer_wins_over_cookie() {
        let parts = parts(&[
            ("authorization", "Bearer tira_abc"),
            ("cookie", "theme=dark; tira_token=tira_cookie"),
        ]);
        assert_eq!(extract_secret(&parts), Some("tira_abc"));
    }

    #[test]
    fn cookie_is_the_fallback_for_pages() {
        let parts = parts(&[("cookie", "theme=dark; tira_token=tira_cookie; x=1")]);
        assert_eq!(extract_secret(&parts), Some("tira_cookie"));
    }

    #[test]
    fn empty_cookie_is_ignored() {
        let parts = parts(&[("cookie", "tira_token=")]);
        assert_eq!(extract_secret(&parts), None);
    }

    #[test]
    fn basic_auth_is_rejected() {
        let parts = parts(&[("authorization", "Basic dGlyYTp0ZXN0")]);
        assert_eq!(extract_secret(&parts), None);
    }
}
