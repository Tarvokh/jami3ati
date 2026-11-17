//! Outbound submission: building and sending a message on behalf of a
//! mailbox.
//!
//! This is the write half of the mail surface. It deliberately mirrors the read
//! module's discipline:
//!
//!   * every attempt — success or failure — is written to `audit_log`;
//!   * a mailbox-scoped web session can only ever speak for the mailbox it owns;
//!   * failures are reported honestly (`502` upstream, `503` unconfigured,
//!     `429` throttled, `400` invalid) and never turned into a fake success.
//!
//! The handler composes the RFC 5322 message through [`tira_smtp::client`]
//! (STARTTLS + AUTH via `lettre`) and submits it to the configured relay. A
//! browser form post gets an HTML screen back (redirect on success, the compose
//! screen with the refusal on failure); the error codes remain for any other
//! caller that prefers a status body.

use axum::extract::{Form, Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use chrono::{Duration as ChronoDuration, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tira_frontend::{pages, MailboxModel, SentNotice};
use tira_smtp::client::{
    build_message, build_message_with, validate_address, MailError, SmtpClient,
};
use uuid::Uuid;

use crate::auth::{Auth, PageAuth};
use crate::read::RequestMeta;
use crate::AppState;
use tira_store::{AuditContext, OutboundQueueEntry, StoreError};

/// Hard ceiling on a composed message body. Fail fast on absurd input; the
/// demo and the pilot never come close.
const SEND_MAX_BODY_BYTES: usize = 65_536;
/// Hard ceiling on recipients per composed message (comma-separated list).
const SEND_MAX_RECIPIENTS: usize = 10;
/// Sends allowed per mailbox per window before throttling kicks in.
const SEND_MAX_PER_WINDOW: u32 = 5;
/// Base backoff between outbound retry attempts, in seconds.
const OUTBOUND_QUEUE_BACKOFF_BASE_SECS: i64 = 30;
/// Upper bound for the outbound retry backoff, in seconds.
const OUTBOUND_QUEUE_BACKOFF_CAP_SECS: i64 = 3_600;
/// How many due outbound sends one worker tick may claim.
const OUTBOUND_QUEUE_BATCH: usize = 8;
/// Default poll interval of the outbound queue worker, in seconds.
const OUTBOUND_QUEUE_DEFAULT_POLL_SECS: u64 = 2;
/// Machine submissions accepted per token per window before throttling.
const API_SEND_MAX_PER_WINDOW: u32 = 120;
/// Recipient ceiling for one machine submission (fan-out is the caller's job).
const API_SEND_MAX_RECIPIENTS: usize = 50;
/// Subject ceiling in Unicode scalar values, not bytes (Arabic is 2 bytes/char).
const API_SEND_MAX_SUBJECT_CHARS: usize = 255;
/// `List-Unsubscribe` is a URI, never prose; 512 bytes is generous.
const API_SEND_MAX_UNSUBSCRIBE_BYTES: usize = 512;
/// Idempotency and correlation keys are opaque identifiers, not free text.
const API_KEY_MIN_LEN: usize = 8;
const API_KEY_MAX_LEN: usize = 128;
/// How long a one-shot send confirmation stays readable after the redirect.
const SENT_NOTICE_TTL_SECS: u64 = 600;
/// Upper bound on retained confirmations, so a send burst cannot grow memory.
const SENT_NOTICE_CAP: usize = 64;

#[derive(Debug)]
struct SentNoticeEntry {
    mailbox_id: Uuid,
    notice: SentNotice,
    created: Instant,
}

/// In-memory store of SMTP-handoff confirmations.
///
/// The redirect carries a random UUID, and the lookup is additionally bound to
/// the mailbox from the authenticated URL. A hand-crafted `?sent=` value can
/// therefore never display a confirmation for a send this server did not make,
/// and the notice expires after [`SENT_NOTICE_TTL_SECS`] instead of lingering in
/// browser history.
#[derive(Debug)]
pub struct SentNotices {
    inner: Mutex<HashMap<Uuid, SentNoticeEntry>>,
}

impl SentNotices {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Records a confirmation and returns the id to put in the redirect URL.
    pub fn issue(&self, mailbox_id: Uuid, notice: SentNotice) -> Uuid {
        let id = Uuid::new_v4();
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard
            .retain(|_, entry| entry.created.elapsed() < Duration::from_secs(SENT_NOTICE_TTL_SECS));
        if guard.len() >= SENT_NOTICE_CAP {
            if let Some(oldest) = guard
                .iter()
                .min_by_key(|(_, entry)| entry.created)
                .map(|(id, _)| *id)
            {
                guard.remove(&oldest);
            }
        }
        guard.insert(
            id,
            SentNoticeEntry {
                mailbox_id,
                notice,
                created: Instant::now(),
            },
        );
        id
    }

    /// Returns the confirmation only for the mailbox it was issued to.
    pub fn get(&self, mailbox_id: Uuid, id: Uuid) -> Option<SentNotice> {
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard
            .retain(|_, entry| entry.created.elapsed() < Duration::from_secs(SENT_NOTICE_TTL_SECS));
        guard
            .get(&id)
            .filter(|entry| entry.mailbox_id == mailbox_id)
            .map(|entry| entry.notice.clone())
    }
}

impl Default for SentNotices {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct SendForm {
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone)]
struct SendRequest {
    /// Comma-joined recipient list. Single-recipient sends are the demo shape;
    /// the cap keeps abuse bounded.
    to: String,
    subject: String,
    body: String,
}

#[derive(Debug)]
pub enum SendError {
    BadRequest(&'static str),
    NotFound(&'static str),
    Throttled,
    RelayMissing,
    Unavailable(String),
    Upstream(String),
}

impl IntoResponse for SendError {
    fn into_response(self) -> Response {
        let throttled = matches!(self, SendError::Throttled);
        let (code, message) = match self {
            SendError::BadRequest(message) => (StatusCode::BAD_REQUEST, message.to_string()),
            SendError::NotFound(message) => (StatusCode::NOT_FOUND, message.to_string()),
            SendError::Throttled => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many sends. please retry later.".to_string(),
            ),
            SendError::RelayMissing => (
                StatusCode::SERVICE_UNAVAILABLE,
                "outbound smtp relay is not configured.".to_string(),
            ),
            SendError::Unavailable(err) => {
                tracing::warn!(%err, "send path failed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage unavailable".to_string(),
                )
            }
            SendError::Upstream(err) => (StatusCode::BAD_GATEWAY, err),
        };

        let mut response = (code, Json(serde_json::json!({ "error": message }))).into_response();
        if throttled {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("600"));
        }
        response
    }
}

impl From<MailError> for SendError {
    fn from(_err: MailError) -> Self {
        SendError::BadRequest("البريد الإلكتروني أو الرسالة غير صالحة")
    }
}

impl From<StoreError> for SendError {
    fn from(err: StoreError) -> Self {
        SendError::Unavailable(err.to_string())
    }
}

/// Validates a composed send. Pure so the boundaries are unit-testable
/// without a transport or a database. Error copy is Arabic because the refusal
/// is shown on the Arabic compose screen.
fn validate(form: &SendForm) -> Result<SendRequest, SendError> {
    let to = form.to.trim();
    if to.is_empty() {
        return Err(SendError::BadRequest("البريد الإلكتروني مطلوب."));
    }
    let recipients: Vec<String> = to
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect();
    if recipients.is_empty() {
        return Err(SendError::BadRequest("البريد الإلكتروني مطلوب."));
    }
    if recipients.len() > SEND_MAX_RECIPIENTS {
        return Err(SendError::BadRequest("عدد المستلمين أكبر من المسموح."));
    }

    let body = form.body.trim();
    if body.is_empty() {
        return Err(SendError::BadRequest("جسم الرسالة مطلوب."));
    }
    if body.len() > SEND_MAX_BODY_BYTES {
        return Err(SendError::BadRequest("الرسالة أطول من المسموح."));
    }

    let subject = form
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Re:");

    Ok(SendRequest {
        to: recipients.join(","),
        subject: subject.to_string(),
        body: body.to_string(),
    })
}

/// Arabic refusal copy for the browser-facing compose screen. The JSON status
/// bodies keep their own honest English/Arabic strings via [`SendError`].
fn compose_error(err: &SendError) -> String {
    match err {
        SendError::BadRequest(message) => (*message).to_string(),
        SendError::Throttled => "محاولات كثيرة. انتظر قبل إعادة المحاولة.".to_string(),
        SendError::Upstream(_) => "تعذّر الإرسال عبر مسار SMTP. حاول مرة أخرى.".to_string(),
        SendError::RelayMissing => "مسار الإرسال غير مهيّأ بعد.".to_string(),
        SendError::NotFound(_) | SendError::Unavailable(_) => {
            "تعذّر معالجة الطلب. حاول مرة أخرى.".to_string()
        }
    }
}

fn compose_screen(
    mailbox: &MailboxModel,
    to: &str,
    subject: &str,
    body: &str,
    err: &SendError,
) -> Response {
    let error = compose_error(err);
    crate::pages::html(
        StatusCode::OK,
        pages::compose(mailbox, to, subject, body, Some(&error)),
    )
}

/// Exponential backoff in seconds before retrying a failed outbound send.
///
/// Grows from the first retry (30s) by doubling on each failed attempt and caps
/// at one hour, so a wedged relay cannot produce an unbounded retry loop.
fn backoff_secs(attempt: i32) -> i64 {
    let shift = attempt.saturating_sub(1).min(12) as u32;
    let value = OUTBOUND_QUEUE_BACKOFF_BASE_SECS * (1i64 << shift);
    value.min(OUTBOUND_QUEUE_BACKOFF_CAP_SECS)
}

/// Poll interval of the outbound queue worker, overridable for tests.
fn outbound_queue_poll_interval() -> std::time::Duration {
    std::env::var("TIRA_OUTBOUND_QUEUE_POLL_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| (1..=60).contains(value))
        .map(std::time::Duration::from_secs)
        .unwrap_or(std::time::Duration::from_secs(
            OUTBOUND_QUEUE_DEFAULT_POLL_SECS,
        ))
}

/// Background outbound queue worker.
///
/// Polls for due sends (`pending`, past `next_attempt_at`), dispatches each one
/// through the configured relay and then either completes the queue row
/// (delivered), reschedules it with backoff (transient failure) or dead-letters
/// it (attempts exhausted). Runs for the lifetime of the process.
pub async fn outbound_worker(state: AppState) {
    let mut tick = tokio::time::interval(outbound_queue_poll_interval());
    loop {
        tick.tick().await;
        outbound_queue_tick(&state).await;
    }
}

/// One worker tick: claim due sends, attempt delivery, resolve the outcome.
async fn outbound_queue_tick(state: &AppState) {
    let Some(store) = state.store.as_ref() else {
        return;
    };
    let Some(relay) = state.smtp_relay.as_ref() else {
        return;
    };

    let entries = match store.claim_due_outbound(OUTBOUND_QUEUE_BATCH).await {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(%err, "outbound queue claim failed");
            return;
        }
    };

    for entry in entries {
        let outcome = deliver_outbound(relay, &entry).await;
        let audit = AuditContext {
            request_id: Uuid::nil(),
            actor: format!("queue:{}", entry.id),
            ip: None,
            user_agent: Some("tira-outbound-worker".to_string()),
        };
        let audit_action = "mail.send";
        let audit_object = format!("mailbox:{}", entry.mailbox_id);
        let audit_metadata = serde_json::json!({
            "subject": entry.subject,
            "attempts": entry.attempts,
        });

        match outcome {
            Ok(()) => {
                if let Err(err) = store.complete_outbound(entry.id).await {
                    tracing::warn!(%err, outbound_id = %entry.id, "outbound queue complete failed");
                }
                if let Err(err) = store
                    .record_auth_event(
                        Some(entry.tenant_id),
                        &audit,
                        audit_action,
                        Some(&audit_object),
                        "ok",
                        audit_metadata,
                    )
                    .await
                {
                    tracing::warn!(%err, "outbound queue audit write failed");
                }
            }
            Err(error) => {
                if let Err(err) = store
                    .record_auth_event(
                        Some(entry.tenant_id),
                        &audit,
                        audit_action,
                        Some(&audit_object),
                        "error",
                        serde_json::json!({ "error": error }),
                    )
                    .await
                {
                    tracing::warn!(%err, "outbound queue audit write failed");
                }
                let next_attempts = entry.attempts;
                if next_attempts < entry.max_attempts {
                    let next_attempt_at =
                        Utc::now() + ChronoDuration::seconds(backoff_secs(next_attempts));
                    if let Err(err) = store
                        .retry_outbound(entry.id, &error, next_attempt_at)
                        .await
                    {
                        tracing::warn!(%err, outbound_id = %entry.id, "outbound queue retry write failed");
                    }
                } else {
                    // Dead letter: the failure was permanent, so the composed
                    // intent becomes a durable bounce record before the row is
                    // closed. The human is informed through the honest error; the
                    // ledger keeps the reason for the compliance trail.
                    if let Err(err) = store.dead_letter_outbound(entry.id, &error).await {
                        tracing::warn!(%err, outbound_id = %entry.id, "outbound queue dead-letter write failed");
                    }
                    let recipient = entry
                        .recipients
                        .split(',')
                        .map(str::trim)
                        .find(|value| !value.is_empty())
                        .unwrap_or(&entry.from_addr)
                        .to_string();
                    if let Err(err) = store
                        .record_bounce(entry.tenant_id, entry.mailbox_id, &recipient, &error, true)
                        .await
                    {
                        tracing::warn!(%err, outbound_id = %entry.id, "outbound queue bounce write failed");
                    }
                    if let Err(err) = store
                        .record_auth_event(
                            Some(entry.tenant_id),
                            &audit,
                            "mail.bounce",
                            Some(&audit_object),
                            "ok",
                            serde_json::json!({ "reason": error, "permanent": true }),
                        )
                        .await
                    {
                        tracing::warn!(%err, "outbound queue bounce audit write failed");
                    }
                }
            }
        }
    }
}

/// Dispatches one queued outbound intent through the configured relay.
async fn deliver_outbound(
    relay: &tira_smtp::client::SmtpRelay,
    entry: &OutboundQueueEntry,
) -> Result<(), String> {
    let client = if relay.starttls {
        SmtpClient::starttls(&relay.host, relay.port, &relay.username, &relay.password)
            .map_err(|err| err.to_string())?
    } else {
        SmtpClient::plaintext(&relay.host, relay.port).map_err(|err| err.to_string())?
    };
    let recipients: Vec<&str> = entry
        .recipients
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();
    let message = build_message_with(
        &entry.from_addr,
        &recipients,
        &entry.subject,
        &entry.body,
        &outbound_headers(entry),
    )
    .map_err(|err| err.to_string())?;
    client.send(message).await.map_err(|err| err.to_string())
}

/// Headers the queue row asks for. The names are literals, so a row can never
/// introduce a header the code did not choose; the values were validated at
/// accept time.
fn outbound_headers(entry: &OutboundQueueEntry) -> Vec<(&'static str, &str)> {
    let mut headers = Vec::new();
    if let Some(value) = entry.correlation_id.as_deref() {
        headers.push(("X-Jami3ati-Notification", value));
    }
    if let Some(value) = entry.list_unsubscribe.as_deref() {
        headers.push(("List-Unsubscribe", value));
        headers.push(("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"));
    }
    if let Some(value) = entry.reply_to.as_deref() {
        headers.push(("Reply-To", value));
    }
    headers
}

/// Machine-callable acceptance of a send.
///
/// This is the Jami3ati contract: the caller hands over a composed *intent*
/// and gets a durable acceptance (`202`), not a delivery claim. The background
/// worker performs the delivery and records the outcome; the caller can poll
/// nothing and still be correct, because the queue never drops the intent
/// silently.
///
/// Idempotency is per tenant: a retried POST with the same `idempotency_key`
/// returns the original queue id with `duplicate: true` instead of composing a
/// second message.
#[derive(Debug, Deserialize)]
pub struct ApiSendRequest {
    /// Required only when the bearer token is tenant-wide; a mailbox-scoped
    /// token already names its mailbox and may omit it (or repeat it).
    #[serde(default)]
    pub mailbox_id: Option<String>,
    #[serde(default)]
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub list_unsubscribe: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiSendResponse {
    pub id: Uuid,
    pub status: &'static str,
    /// True when the key already existed: the response describes the original
    /// acceptance, not a new one.
    pub duplicate: bool,
}

/// Machine-facing refusals. The copy is English on purpose: `/api/v1` is the
/// programmatic surface (the read API's errors are English too), while the
/// browser compose screen keeps its Arabic copy.
#[derive(Debug)]
pub enum ApiSendError {
    BadRequest(&'static str),
    NotFound,
    Throttled,
    RelayMissing,
    Unavailable(String),
}

impl IntoResponse for ApiSendError {
    fn into_response(self) -> Response {
        let throttled = matches!(self, ApiSendError::Throttled);
        let (code, message) = match self {
            ApiSendError::BadRequest(message) => (StatusCode::BAD_REQUEST, message.to_string()),
            // Same body for "unknown" and "another tenant's": the id space must
            // not be enumerable.
            ApiSendError::NotFound => (StatusCode::NOT_FOUND, "mailbox not found".to_string()),
            ApiSendError::Throttled => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many sends for this token; retry later".to_string(),
            ),
            ApiSendError::RelayMissing => (
                StatusCode::SERVICE_UNAVAILABLE,
                "outbound relay is not configured; refusing to accept a send that cannot be delivered"
                    .to_string(),
            ),
            ApiSendError::Unavailable(err) => {
                tracing::warn!(%err, "api send path failed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage unavailable".to_string(),
                )
            }
        };

        let mut response = (code, Json(serde_json::json!({ "error": message }))).into_response();
        if throttled {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("600"));
        }
        response
    }
}

/// The validated shape handed to the store. Every field has already passed the
/// closed-set rules below, so the worker can rebuild the message later without
/// discovering a problem at delivery time.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedApiSend {
    recipients: String,
    subject: String,
    body: String,
    idempotency_key: String,
    correlation_id: Option<String>,
    list_unsubscribe: Option<String>,
    reply_to: Option<String>,
}

/// `[A-Za-z0-9._:-]`: the shape of a UUID, a namespaced id, or a hex digest.
/// No spaces, no quotes, nothing a header could interpret.
fn is_key_charset(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn optional_key(value: Option<&str>) -> Result<Option<String>, ApiSendError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > API_KEY_MAX_LEN || !is_key_charset(value) {
        return Err(ApiSendError::BadRequest(
            "key fields must be 1-128 chars of [A-Za-z0-9._:-]",
        ));
    }
    Ok(Some(value.to_string()))
}

/// Pure validation: no store, no relay, no clock. Kept separate so every
/// boundary is unit-testable without a database.
fn validate_api(request: &ApiSendRequest) -> Result<ValidatedApiSend, ApiSendError> {
    let subject = request.subject.trim();
    if subject.is_empty() {
        return Err(ApiSendError::BadRequest("subject is required"));
    }
    if subject.chars().count() > API_SEND_MAX_SUBJECT_CHARS {
        return Err(ApiSendError::BadRequest(
            "subject is longer than 255 characters",
        ));
    }
    if subject.contains(['\r', '\n']) {
        return Err(ApiSendError::BadRequest("subject must be a single line"));
    }

    let body = request.text.trim();
    if body.is_empty() {
        return Err(ApiSendError::BadRequest("text is required"));
    }
    if body.len() > SEND_MAX_BODY_BYTES {
        return Err(ApiSendError::BadRequest("text is longer than 64 KiB"));
    }

    if request.to.is_empty() {
        return Err(ApiSendError::BadRequest(
            "to must contain at least one address",
        ));
    }
    if request.to.len() > API_SEND_MAX_RECIPIENTS {
        return Err(ApiSendError::BadRequest(
            "to contains more than 50 addresses",
        ));
    }
    let mut recipients = Vec::with_capacity(request.to.len());
    for raw in &request.to {
        let address = raw.trim();
        if address.is_empty() {
            return Err(ApiSendError::BadRequest("to contains an empty address"));
        }
        // The queue stores recipients as a comma-separated list, so a comma
        // inside an address would silently split it later.
        if address.contains([',', '\r', '\n']) {
            return Err(ApiSendError::BadRequest(
                "recipient address contains a separator",
            ));
        }
        if validate_address(address).is_err() {
            return Err(ApiSendError::BadRequest("recipient address is invalid"));
        }
        recipients.push(address.to_ascii_lowercase());
    }

    let idempotency_key = request.idempotency_key.trim();
    if idempotency_key.len() < API_KEY_MIN_LEN
        || idempotency_key.len() > API_KEY_MAX_LEN
        || !is_key_charset(idempotency_key)
    {
        return Err(ApiSendError::BadRequest(
            "idempotency_key must be 8-128 chars of [A-Za-z0-9._:-]",
        ));
    }

    let correlation_id = optional_key(request.correlation_id.as_deref())?;

    let list_unsubscribe = match request
        .list_unsubscribe
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            let well_formed = value.len() <= API_SEND_MAX_UNSUBSCRIBE_BYTES
                && value.starts_with('<')
                && value.ends_with('>')
                && (value.starts_with("<https://") || value.starts_with("<mailto:"))
                && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
            if !well_formed {
                return Err(ApiSendError::BadRequest(
                    "list_unsubscribe must be <https://...> or <mailto:...>",
                ));
            }
            Some(value.to_string())
        }
        None => None,
    };

    let reply_to = match request
        .reply_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            if !value.is_ascii() || value.contains(['\r', '\n']) {
                return Err(ApiSendError::BadRequest("reply_to must be a plain address"));
            }
            if validate_address(value).is_err() {
                return Err(ApiSendError::BadRequest("reply_to is not a valid address"));
            }
            Some(value.to_string())
        }
        None => None,
    };

    Ok(ValidatedApiSend {
        recipients: recipients.join(","),
        subject: subject.to_string(),
        body: body.to_string(),
        idempotency_key: idempotency_key.to_string(),
        correlation_id,
        list_unsubscribe,
        reply_to,
    })
}

/// Headers for a validated machine send. Mirrors [`outbound_headers`] for the
/// owned queue row; names stay literals so the closed set is visible in one
/// place.
fn api_headers(send: &ValidatedApiSend) -> Vec<(&'static str, &str)> {
    let mut headers = Vec::new();
    if let Some(value) = send.correlation_id.as_deref() {
        headers.push(("X-Jami3ati-Notification", value));
    }
    if let Some(value) = send.list_unsubscribe.as_deref() {
        headers.push(("List-Unsubscribe", value));
        headers.push(("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"));
    }
    if let Some(value) = send.reply_to.as_deref() {
        headers.push(("Reply-To", value));
    }
    headers
}

/// `POST /api/v1/outbound`
///
/// Accepts a composed send into the durable queue. Returns `202` with the queue
/// id; never claims delivery. A missing relay is a `503` at accept time,
/// because accepting a message that can never be delivered would be a lie.
pub async fn send_api(
    State(state): State<AppState>,
    auth: Auth,
    meta: RequestMeta,
    Json(request): Json<ApiSendRequest>,
) -> Response {
    let Some(store) = state.store.as_ref() else {
        return ApiSendError::Unavailable("no database configured".to_string()).into_response();
    };

    // A mailbox-scoped token may only speak for its own mailbox; a tenant-wide
    // token must name one explicitly. Never guess an envelope sender.
    let mailbox_id = match auth.0.mailbox_id {
        Some(scoped) => {
            match request
                .mailbox_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(requested) => match Uuid::parse_str(requested) {
                    Ok(parsed) if parsed == scoped => scoped,
                    Ok(_) => return ApiSendError::NotFound.into_response(),
                    Err(_) => {
                        return ApiSendError::BadRequest("mailbox_id is not a valid UUID")
                            .into_response()
                    }
                },
                None => scoped,
            }
        }
        None => {
            let Some(requested) = request
                .mailbox_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return ApiSendError::BadRequest("mailbox_id is required for a tenant-wide token")
                    .into_response();
            };
            match Uuid::parse_str(requested) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return ApiSendError::BadRequest("mailbox_id is not a valid UUID")
                        .into_response()
                }
            }
        }
    };

    if state.smtp_relay.is_none() {
        return ApiSendError::RelayMissing.into_response();
    }

    let validated = match validate_api(&request) {
        Ok(validated) => validated,
        Err(err) => return err.into_response(),
    };

    let mailbox = match store.get_mailbox(mailbox_id).await {
        Ok(Some(mailbox)) if mailbox.tenant_id == auth.0.tenant_id => mailbox,
        // `get_mailbox` runs as the owner role (RLS bypassed), so the tenant
        // comparison above is the actual boundary, not a redundant check.
        Ok(_) => return ApiSendError::NotFound.into_response(),
        Err(err) => return ApiSendError::Unavailable(err.to_string()).into_response(),
    };

    let limiter_key = format!("api-send:{}", auth.0.token_id);
    if !state
        .login_limiter
        .try_attempt_capped(&limiter_key, API_SEND_MAX_PER_WINDOW)
    {
        return ApiSendError::Throttled.into_response();
    }

    // Prove now that the worker can rebuild this exact message later: if the
    // acceptance succeeds, delivery must not fail on a field we could have
    // rejected here.
    let recipient_refs: Vec<&str> = validated.recipients.split(',').collect();
    if let Err(err) = build_message_with(
        &mailbox.address,
        &recipient_refs,
        &validated.subject,
        &validated.body,
        &api_headers(&validated),
    ) {
        tracing::warn!(%err, "api send rejected: message cannot be built");
        return ApiSendError::BadRequest("message cannot be built from these fields")
            .into_response();
    }

    let (outbound_id, created) = match store
        .enqueue_outbound_api(
            auth.0.tenant_id,
            mailbox_id,
            &mailbox.address,
            &validated.recipients,
            &validated.subject,
            &validated.body,
            &validated.idempotency_key,
            validated.correlation_id.as_deref(),
            validated.list_unsubscribe.as_deref(),
            validated.reply_to.as_deref(),
        )
        .await
    {
        Ok(result) => result,
        Err(err) => return ApiSendError::Unavailable(err.to_string()).into_response(),
    };

    let audit = meta.audit(auth.0.token_id);
    if let Err(err) = store
        .record_auth_event(
            Some(auth.0.tenant_id),
            &audit,
            "mail.send.accepted",
            Some(&format!("mailbox:{mailbox_id}")),
            "ok",
            serde_json::json!({
                "outbound_id": outbound_id,
                "duplicate": !created,
                "correlation_id": validated.correlation_id,
            }),
        )
        .await
    {
        tracing::warn!(%err, "api send acceptance audit write failed");
    }

    tracing::info!(
        outbound_id = %outbound_id,
        duplicate = !created,
        "api send accepted"
    );

    (
        StatusCode::ACCEPTED,
        Json(ApiSendResponse {
            id: outbound_id,
            status: "queued",
            duplicate: !created,
        }),
    )
        .into_response()
}

/// `POST /mail/{mailbox_id}/send`
///
/// Composes and submits a message on behalf of the authenticated mailbox. The
/// reply body is authored by the human, so the only automation is structural:
/// envelope sender = the mailbox owner, subject default = `Re:`.
pub async fn send(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    Path(mailbox_id): Path<String>,
    Form(form): Form<SendForm>,
) -> Response {
    let Some(store) = state.store.as_ref() else {
        return crate::pages::service_unavailable_response("no database configured");
    };

    let mailbox_id = match Uuid::parse_str(&mailbox_id) {
        Ok(id) => id,
        Err(_) => {
            return crate::pages::problem(
                StatusCode::BAD_REQUEST,
                "بيانات غير صالحة",
                "هذا الطلب لا يحتوي على معرّف صندوق صالح.",
                "المعرّف المطلوب ناقص أو غير صالح، لذلك لا يمكن الإنشاء.",
            );
        }
    };

    // A mailbox-scoped session can only speak for the mailbox it owns; a
    // tenant-wide token cannot pick one at all, so both shapes are refused
    // here rather than silently picking an envelope sender.
    if auth.0.mailbox_id != Some(mailbox_id) {
        return crate::pages::problem(
            StatusCode::NOT_FOUND,
            "غير موجود",
            "لا يوجد صندوق بهذا المعرّف.",
            "قد يكون المعرّف خاطئًا أو تابعًا لمستأجر آخر. الردّ واحد في الحالتين حتى لا يمكن تخمين المعرّفات.",
        );
    }

    let audit = meta.audit(auth.0.token_id);
    let page = match store
        .read_mailbox_page(
            mailbox_id,
            auth.0.tenant_id,
            auth.0.mailbox_id,
            1,
            0,
            &audit,
        )
        .await
    {
        Ok(Some(page)) => page,
        Ok(None) => {
            return crate::pages::problem(
                StatusCode::NOT_FOUND,
                "غير موجود",
                "لا يوجد صندوق بهذا المعرّف.",
                "قد يكون المعرّف خاطئًا أو تابعًا لمستأجر آخر. الردّ واحد في الحالتين حتى لا يمكن تخمين المعرّفات.",
            );
        }
        Err(err) => {
            tracing::warn!(%err, "send path mailbox read failed");
            return crate::pages::service_unavailable_response("storage unavailable");
        }
    };

    let mailbox = MailboxModel {
        id: mailbox_id.to_string(),
        address: page.mailbox.address.clone(),
        display_name: page.mailbox.display_name.clone(),
        total: page.total,
        used_bytes: page.mailbox.used_bytes,
        quota_bytes: page.mailbox.quota_bytes,
    };

    let validated = validate(&form);
    let (to, subject, body) = match validated {
        Ok(request) => (request.to, request.subject, request.body),
        Err(err) => {
            let to = form.to.trim().to_string();
            let subject = form
                .subject
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            let body = form.body.trim().to_string();
            return compose_screen(&mailbox, &to, &subject, &body, &err);
        }
    };

    let ip = meta
        .ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let limiter_key = format!("mailbox-send:{mailbox_id}|{ip}");
    if !state
        .login_limiter
        .try_attempt_capped(&limiter_key, SEND_MAX_PER_WINDOW)
    {
        return compose_screen(&mailbox, &to, &subject, &body, &SendError::Throttled);
    }

    let relay = match state.smtp_relay.as_ref() {
        Some(relay) => relay,
        None => return compose_screen(&mailbox, &to, &subject, &body, &SendError::RelayMissing),
    };
    let client = if relay.starttls {
        match SmtpClient::starttls(&relay.host, relay.port, &relay.username, &relay.password) {
            Ok(client) => client,
            Err(err) => {
                return compose_screen(
                    &mailbox,
                    &to,
                    &subject,
                    &body,
                    &SendError::Upstream(err.to_string()),
                );
            }
        }
    } else {
        match SmtpClient::plaintext(&relay.host, relay.port) {
            Ok(client) => client,
            Err(err) => {
                return compose_screen(
                    &mailbox,
                    &to,
                    &subject,
                    &body,
                    &SendError::Upstream(err.to_string()),
                );
            }
        }
    };

    let from = mailbox.address.clone();
    let recipient_refs: Vec<&str> = to
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    let message = match build_message(&from, &recipient_refs, &subject, &body) {
        Ok(message) => message,
        Err(err) => {
            tracing::warn!(%err, "compose failed: invalid address or message");
            return compose_screen(
                &mailbox,
                &to,
                &subject,
                &body,
                &SendError::BadRequest("البريد الإلكتروني أو الرسالة غير صالحة"),
            );
        }
    };

    if let Err(err) = client.send(message).await {
        // Durability: the composed intent is parked in the outbound queue so
        // the retry worker can replay it with backoff instead of dropping it.
        // The human still gets the honest refusal right now (Fail-Fast).
        if let Err(queue_err) = store
            .enqueue_outbound(
                auth.0.tenant_id,
                mailbox_id,
                &mailbox.address,
                &to,
                &subject,
                &body,
            )
            .await
        {
            tracing::warn!(%queue_err, "outbound queue enqueue failed");
        }

        if let Err(audit_err) = store
            .record_auth_event(
                Some(auth.0.tenant_id),
                &audit,
                "mail.send",
                Some(&format!("mailbox:{mailbox_id}")),
                "error",
                serde_json::json!({ "error": err.to_string() }),
            )
            .await
        {
            tracing::warn!(%audit_err, "send audit write failed");
        }
        return compose_screen(
            &mailbox,
            &to,
            &subject,
            &body,
            &SendError::Upstream(err.to_string()),
        );
    }

    if let Err(err) = store
        .record_auth_event(
            Some(auth.0.tenant_id),
            &audit,
            "mail.send",
            Some(&format!("mailbox:{mailbox_id}")),
            "ok",
            serde_json::json!({ "subject": subject }),
        )
        .await
    {
        tracing::warn!(%err, "send audit write failed");
    }

    state.login_limiter.clear(&limiter_key);

    // The redirect carries an unguessable id, not the facts themselves: the
    // confirmation is re-read from this process's memory and bound to the
    // authenticated mailbox. It says "the relay accepted it", nothing more.
    let notice_id = state.sent_notices.issue(
        mailbox_id,
        SentNotice {
            to: to.clone(),
            subject: subject.clone(),
            accepted_at: tira_frontend::format::full_stamp(
                Utc::now().with_timezone(&Local).fixed_offset(),
            ),
            relay: format!("{}:{}", relay.host, relay.port),
        },
    );

    tracing::info!(mailbox_id = %mailbox_id, "mail send succeeded");
    Redirect::to(&format!("/mail/{mailbox_id}?sent={notice_id}")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(to: &str, subject: Option<&str>, body: &str) -> SendForm {
        SendForm {
            to: to.to_string(),
            subject: subject.map(|s| s.to_string()),
            body: body.to_string(),
        }
    }

    fn notice(subject: &str) -> SentNotice {
        SentNotice {
            to: "aicha@uca.ma".to_string(),
            subject: subject.to_string(),
            accepted_at: "2026-11-17 13:00".to_string(),
            relay: "127.0.0.1:1025".to_string(),
        }
    }

    #[test]
    fn a_sent_notice_is_bound_to_its_mailbox() {
        let notices = SentNotices::new();
        let mailbox = Uuid::new_v4();
        let sibling = Uuid::new_v4();

        let id = notices.issue(mailbox, notice("Re: Salam"));

        let found = notices.get(mailbox, id).expect("own mailbox must read it");
        assert_eq!(found.to, "aicha@uca.ma");
        assert_eq!(found.relay, "127.0.0.1:1025");
        // A sibling mailbox holding the same URL must not see the notice, and a
        // random id must never resolve to one.
        assert!(notices.get(sibling, id).is_none());
        assert!(notices.get(mailbox, Uuid::new_v4()).is_none());
    }

    #[test]
    fn the_notice_store_is_capped_and_evicts_the_oldest() {
        let notices = SentNotices::new();
        let mailbox = Uuid::new_v4();
        let ids: Vec<Uuid> = (0..SENT_NOTICE_CAP + 5)
            .map(|i| notices.issue(mailbox, notice(&format!("subject {i}"))))
            .collect();

        assert!(notices.get(mailbox, ids[0]).is_none());
        assert!(notices
            .get(mailbox, *ids.last().expect("at least one id"))
            .is_some());
    }

    #[test]
    fn accepts_a_well_formed_composition() {
        let request = validate(&form(
            "Aicha@uca.ma",
            Some("Re: Tira test"),
            "السلام، هذا رد.",
        ))
        .expect("valid form must pass");
        assert_eq!(request.to, "aicha@uca.ma");
        assert_eq!(request.subject, "Re: Tira test");
        assert_eq!(request.body, "السلام، هذا رد.");
    }

    #[test]
    fn defaults_the_subject_to_a_reply_marker() {
        let request = validate(&form("aicha@uca.ma", None, "hello")).expect("valid form");
        assert_eq!(request.subject, "Re:");
    }

    #[test]
    fn rejects_an_empty_recipient() {
        assert!(matches!(
            validate(&form("   ", None, "hello")),
            Err(SendError::BadRequest("البريد الإلكتروني مطلوب."))
        ));
    }

    #[test]
    fn rejects_an_empty_body() {
        assert!(matches!(
            validate(&form("aicha@uca.ma", None, "   ")),
            Err(SendError::BadRequest("جسم الرسالة مطلوب."))
        ));
    }

    #[test]
    fn rejects_an_oversized_body() {
        let long = "x".repeat(SEND_MAX_BODY_BYTES + 1);
        assert!(matches!(
            validate(&form("aicha@uca.ma", None, &long)),
            Err(SendError::BadRequest("الرسالة أطول من المسموح."))
        ));
    }

    #[test]
    fn rejects_too_many_recipients() {
        let many = (0..SEND_MAX_RECIPIENTS + 1)
            .map(|i| format!("user{i}@tira.test"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(matches!(
            validate(&form(&many, None, "hello")),
            Err(SendError::BadRequest("عدد المستلمين أكبر من المسموح."))
        ));
    }

    #[test]
    fn throttled_answers_429_with_retry_after() {
        let response = SendError::Throttled.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("600")
        );
    }

    #[test]
    fn upstream_send_failure_answers_502() {
        let response = SendError::Upstream("dial tcp failed".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn bad_request_answers_400() {
        let response = SendError::BadRequest("البريد الإلكتروني مطلوب.").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn missing_relay_answers_503() {
        let response = SendError::RelayMissing.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn outbound_backoff_grows_exponentially_from_the_first_retry() {
        assert_eq!(backoff_secs(1), 30);
        assert_eq!(backoff_secs(2), 60);
        assert_eq!(backoff_secs(3), 120);
        assert_eq!(backoff_secs(4), 240);
    }

    #[test]
    fn outbound_backoff_is_capped() {
        assert_eq!(backoff_secs(40), OUTBOUND_QUEUE_BACKOFF_CAP_SECS);
    }

    fn api_request() -> ApiSendRequest {
        ApiSendRequest {
            mailbox_id: None,
            to: vec!["Etudiant@Example.MA".to_string()],
            subject: "تحديث موعد الترشيح".to_string(),
            text: "نص الرسالة".to_string(),
            idempotency_key: "jami3ati:notif:0001".to_string(),
            correlation_id: Some("6f1c2a4e-1111-4222-8333-444455556666".to_string()),
            list_unsubscribe: Some("<https://jami3ati.ma/u/abc.def>".to_string()),
            reply_to: Some("no-reply@jami3ati.ma".to_string()),
        }
    }

    #[test]
    fn api_accepts_a_well_formed_request_and_normalizes_recipients() {
        let validated = validate_api(&api_request()).expect("valid request");
        assert_eq!(validated.recipients, "etudiant@example.ma");
        assert_eq!(validated.subject, "تحديث موعد الترشيح");
        assert_eq!(validated.idempotency_key, "jami3ati:notif:0001");
        assert!(validated.correlation_id.is_some());
    }

    #[test]
    fn api_rejects_a_multiline_subject() {
        let mut request = api_request();
        request.subject = "ok\r\nBcc: evil@example.com".to_string();
        assert!(matches!(
            validate_api(&request),
            Err(ApiSendError::BadRequest("subject must be a single line"))
        ));
    }

    #[test]
    fn api_rejects_a_key_outside_the_closed_charset() {
        for key in [
            "short",
            "has space here",
            "quote\"inside",
            "newline\ninside",
        ] {
            let mut request = api_request();
            request.idempotency_key = key.to_string();
            assert!(
                matches!(validate_api(&request), Err(ApiSendError::BadRequest(_))),
                "expected refusal for key {key:?}"
            );
        }
    }

    #[test]
    fn api_rejects_a_bare_url_or_non_ascii_unsubscribe() {
        for value in [
            "https://jami3ati.ma/u/abc",
            "<https://jami3ati.ma/u/abc",
            "<mailto:unsub@jami3ati.ma> extra",
            "<https://jami3ati.ma/u/قيمة>",
        ] {
            let mut request = api_request();
            request.list_unsubscribe = Some(value.to_string());
            assert!(
                matches!(validate_api(&request), Err(ApiSendError::BadRequest(_))),
                "expected refusal for {value:?}"
            );
        }
    }

    #[test]
    fn api_rejects_a_recipient_containing_a_comma() {
        let mut request = api_request();
        request.to = vec!["a@tira.test,b@tira.test".to_string()];
        assert!(matches!(
            validate_api(&request),
            Err(ApiSendError::BadRequest(
                "recipient address contains a separator"
            ))
        ));
    }

    #[test]
    fn api_rejects_an_invalid_reply_to() {
        let mut request = api_request();
        request.reply_to = Some("not-an-address".to_string());
        assert!(matches!(
            validate_api(&request),
            Err(ApiSendError::BadRequest("reply_to is not a valid address"))
        ));
    }

    #[test]
    fn api_headers_are_the_closed_set() {
        let validated = validate_api(&api_request()).expect("valid request");
        let headers = api_headers(&validated);
        assert_eq!(
            headers,
            vec![
                (
                    "X-Jami3ati-Notification",
                    "6f1c2a4e-1111-4222-8333-444455556666"
                ),
                ("List-Unsubscribe", "<https://jami3ati.ma/u/abc.def>"),
                ("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"),
                ("Reply-To", "no-reply@jami3ati.ma"),
            ]
        );
    }

    #[test]
    fn api_refusals_use_honest_status_codes() {
        assert_eq!(
            ApiSendError::BadRequest("x").into_response().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiSendError::NotFound.into_response().status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiSendError::RelayMissing.into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let throttled = ApiSendError::Throttled.into_response();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            throttled
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("600")
        );
    }
}
