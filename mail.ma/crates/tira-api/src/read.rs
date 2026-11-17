//! Read-only HTTP access to stored mail.
//!
//! This is deliberately the smallest possible door onto the storage layer:
//! list a mailbox, fetch one raw message, and render a server-side HTML page.
//!
//! Every handler here requires a bearer token ([`crate::auth`]). The token
//! carries a tenant and optionally one mailbox; the store drops the database
//! connection to `tira_app` for the duration of the request, so PostgreSQL's
//! row-level security — not a hand-written predicate — decides which rows are
//! visible. Cross-tenant and unknown ids both return 404, so ids cannot be
//! probed, and every attempt is written to `audit_log`.
//!
//! What it is **not**: IMAP, JMAP, roles, or per-message ACLs. A tenant-wide
//! token can read every mailbox in its tenant by design; issue mailbox-scoped
//! tokens when that is too broad.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tira_store::{AuditContext, StoredMessage};
use uuid::Uuid;

use crate::auth::Auth;
use crate::AppState;

/// Hard ceiling on a page of results. The listing is a debugging and pilot
/// surface, not a sync protocol; IMAP/JMAP will paginate differently.
const MAX_LIMIT: i64 = 200;
const DEFAULT_LIMIT: i64 = 50;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(&'static str),
    NotFound(&'static str),
    /// The database is configured but did not answer. Distinct from 404 so a
    /// client retries instead of concluding the message never existed.
    Unavailable(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.to_string()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.to_string()),
            ApiError::Unavailable(err) => {
                tracing::warn!(%err, "read path failed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage unavailable".to_string(),
                )
            }
        };
        (code, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl From<tira_store::StoreError> for ApiError {
    fn from(err: tira_store::StoreError) -> Self {
        ApiError::Unavailable(err.to_string())
    }
}

/// Per-request metadata for the audit trail.
///
/// The request id comes from `x-request-id` when the caller supplies a valid
/// UUID, otherwise it is generated here. A caller cannot inject arbitrary text
/// into the audit log: anything that is not a UUID is replaced.
#[derive(Debug, Clone)]
pub struct RequestMeta {
    pub request_id: Uuid,
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

impl RequestMeta {
    /// Shared with the HTML pages so both surfaces write the same audit shape.
    pub(crate) fn audit(&self, token_id: Uuid) -> AuditContext {
        AuditContext {
            request_id: self.request_id,
            // Never the secret: the trail must be useful without becoming a
            // second place a credential can leak from.
            actor: format!("token:{token_id}"),
            ip: self.ip,
            user_agent: self.user_agent.clone(),
        }
    }
}

impl FromRequestParts<AppState> for RequestMeta {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let request_id = parts
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(Uuid::new_v4);

        // Only present when the process is served with
        // `into_make_service_with_connect_info::<SocketAddr>()`. A forwarded
        // header is deliberately not trusted here.
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip());

        let user_agent = parts
            .headers
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        Ok(Self {
            request_id,
            ip,
            user_agent,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct MessageSummary {
    pub id: Uuid,
    pub subject: Option<String>,
    pub from: String,
    pub size_bytes: i64,
    pub received_at: DateTime<Utc>,
}

impl From<&StoredMessage> for MessageSummary {
    fn from(msg: &StoredMessage) -> Self {
        Self {
            id: msg.id,
            subject: msg.subject.clone(),
            from: msg.from_addr.clone(),
            size_bytes: msg.size_bytes,
            received_at: msg.received_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MessageList {
    pub mailbox_id: Uuid,
    pub address: String,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub messages: Vec<MessageSummary>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

fn store(state: &AppState) -> Result<&tira_store::PgStore, ApiError> {
    state
        .store
        .as_ref()
        .ok_or_else(|| ApiError::Unavailable("no database configured".to_string()))
}

fn parse_uuid(raw: &str, what: &'static str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(what))
}

/// `GET /api/v1/mailboxes/{id}/messages?limit=&offset=`
pub async fn list_messages(
    State(state): State<AppState>,
    auth: Auth,
    meta: RequestMeta,
    Path(mailbox_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<MessageList>, ApiError> {
    let mailbox_id = parse_uuid(&mailbox_id, "mailbox id must be a UUID")?;
    let store = store(&state)?;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);

    // One transaction: resolve the mailbox, read the page, append the audit
    // row. `None` merges "wrong tenant", "wrong mailbox scope", and "does not
    // exist" into one 404.
    let page = store
        .read_mailbox_page(
            mailbox_id,
            auth.0.tenant_id,
            auth.0.mailbox_id,
            limit,
            offset,
            &meta.audit(auth.0.token_id),
        )
        .await?
        .ok_or(ApiError::NotFound("no such mailbox"))?;

    Ok(Json(MessageList {
        mailbox_id,
        address: page.mailbox.address,
        total: page.total,
        limit,
        offset,
        messages: page.messages.iter().map(MessageSummary::from).collect(),
    }))
}

/// `GET /api/v1/messages/{id}/raw` — the exact bytes accepted over SMTP.
///
/// Resolved by message id, not blob key: the blob key is an internal
/// content-addressed name and exposing it would leak the storage layout.
pub async fn raw_message(
    State(state): State<AppState>,
    auth: Auth,
    meta: RequestMeta,
    Path(message_id): Path<String>,
) -> Result<Response, ApiError> {
    let message_id = parse_uuid(&message_id, "message id must be a UUID")?;
    let store = store(&state)?;

    let bytes = store
        .read_message_bytes(
            message_id,
            auth.0.tenant_id,
            auth.0.mailbox_id,
            &meta.audit(auth.0.token_id),
        )
        .await?
        .ok_or(ApiError::NotFound("no such message"))?;

    Ok((
        [
            (header::CONTENT_TYPE, "message/rfc822"),
            // Stored mail is untrusted input. Never let a browser render it
            // inline as HTML.
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"message.eml\"",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}
