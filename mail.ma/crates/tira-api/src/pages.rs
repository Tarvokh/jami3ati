//! Server-rendered mail UI.
//!
//! These handlers are the only place where a browser session is turned into
//! mailbox HTML. They reuse the exact store entry points the JSON API uses, so
//! RLS, token scope, and audit rows are identical on both surfaces — the UI is
//! not a second, weaker door.
//!
//! Failure pages are deliberate:
//!   * malformed id        -> 400 `problem`
//!   * unknown / not yours -> 404 `problem`, same text for both
//!   * storage down        -> 503 `service_unavailable`
//!
//! None of them render JSON at the user, and none of them guess a mailbox.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use chrono::{Duration, Local};
use serde::Deserialize;
use tira_frontend::{
    format, i18n::Locale, pages, InboxModel, MailboxModel, MessageModel, ProfileModel,
    ReadingMessage, ReadingModel, SessionModel,
};
use tira_store::{AuditContext, MailboxProfile, PgStore, StoredMessage};
use uuid::Uuid;

use crate::auth::{secret_from_headers, PageAuth};
use crate::locale::{self, LangQuery};
use crate::read::RequestMeta;
use crate::AppState;

/// A message larger than this is not opened just to build a one-line preview.
/// The list still renders it — only the snippet is omitted, which is honest.
const SNIPPET_MAX_BYTES: i64 = 262_144;

/// One page of the inbox. The JSON API keeps its own 50-row default; the HTML
/// view can afford a longer scroll before pagination exists.
const INBOX_LIMIT: i64 = 100;

/// Password policy for *new* Tira control-plane credentials. Login itself only
/// requires a non-empty password, because an existing credential may predate a
/// policy change and locking it out would be worse than accepting it.
pub(crate) const MIN_PASSWORD_CHARS: usize = 12;
pub(crate) const MAX_PASSWORD_CHARS: usize = 128;
const MAX_DISPLAY_NAME_CHARS: usize = 80;

/// Web sessions are mailbox-scoped API tokens with a fixed lifetime. The label
/// makes them distinguishable from operator-issued tokens in the profile list.
pub(crate) const SESSION_LABEL: &str = "session:web";
pub(crate) const SESSION_TTL_DAYS: i64 = 7;

/// The local part of the address is never a good password. This is a cheap
/// check, not a composition rule: length is the property that matters.
const MIN_LOGIN_EMAIL_CHARS: usize = 3;
const MAX_LOGIN_EMAIL_CHARS: usize = 320;

pub(crate) fn html(status: StatusCode, markup: tira_frontend::Markup) -> Response {
    let mut response = (status, Html(markup.into_string())).into_response();
    // Mailbox contents, the session list, and even the login form must not be
    // retained by a browser or an intermediary. `no-store` is the boundary;
    // `Vary: Cookie` is belt-and-braces for any cache that ignores it, so a
    // shared machine never serves one person's inbox to the next.
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::VARY, HeaderValue::from_static("Cookie"));
    response
}

/// Why a page could not be built. The copy comes from the locale's shell table
/// so a French page never renders an Arabic failure sentence, and the status
/// code still says what happened.
pub(crate) enum ProblemKind {
    BadLink,
    BadMessageLink,
    BadComposeRequest,
    MissingMailbox,
    MissingMessage,
    WrongMailbox,
    DeletedMailbox,
}

pub(crate) fn problem(status: StatusCode, locale: Locale, kind: ProblemKind) -> Response {
    let text = locale.shell();
    let (eyebrow, title, message) = match kind {
        ProblemKind::BadLink => (
            text.problem_bad_link_eyebrow,
            text.problem_bad_link_title,
            text.problem_bad_link_message,
        ),
        ProblemKind::BadMessageLink => (
            text.problem_bad_link_eyebrow,
            text.problem_bad_link_title,
            text.problem_bad_message_link_message,
        ),
        ProblemKind::BadComposeRequest => (
            text.problem_bad_request_eyebrow,
            text.problem_bad_request_title,
            text.problem_bad_request_message,
        ),
        ProblemKind::MissingMailbox => (
            text.problem_not_found_eyebrow,
            text.problem_missing_mailbox,
            text.problem_check_link,
        ),
        ProblemKind::MissingMessage => (
            text.problem_not_found_eyebrow,
            text.problem_missing_message,
            text.problem_check_link,
        ),
        ProblemKind::WrongMailbox => (
            text.problem_not_found_eyebrow,
            text.problem_wrong_mailbox,
            text.problem_wrong_mailbox_message,
        ),
        ProblemKind::DeletedMailbox => (
            text.problem_not_found_eyebrow,
            text.problem_missing_mailbox,
            text.problem_deleted,
        ),
    };
    html(status, pages::problem(locale, eyebrow, title, message))
}

pub(crate) fn unavailable(locale: Locale, err: impl std::fmt::Display) -> Response {
    tracing::warn!(%err, "page render failed");
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        pages::service_unavailable(locale, "storage unavailable"),
    )
}

pub(crate) fn no_store(locale: Locale) -> Response {
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        pages::service_unavailable(locale, "no database configured"),
    )
}

/// Public wrapper used by the auth extractor's rejection path. Kept next to the
/// other page renderers so the 503 body stays identical everywhere.
pub fn service_unavailable_response(locale: Locale, detail: &str) -> Response {
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        pages::service_unavailable(locale, detail),
    )
}

/// Writes the `?lang=` choice back as a cookie so the next POST — which carries
/// no query string — renders in the same language.
pub(crate) fn persist_locale(mut response: Response, lang: Option<&str>) -> Response {
    if let Some(chosen) = lang.and_then(Locale::from_code) {
        locale::set_cookie(&mut response, chosen, cookie_secure());
    }
    response
}

fn server_time() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

fn mailbox_model(page: &tira_store::MailboxPage) -> MailboxModel {
    MailboxModel {
        id: page.mailbox.id.to_string(),
        address: page.mailbox.address.clone(),
        display_name: page.mailbox.display_name.clone(),
        total: page.total,
        used_bytes: page.mailbox.used_bytes,
        quota_bytes: page.mailbox.quota_bytes,
    }
}

async fn snippet_for(store: &PgStore, message: &StoredMessage) -> Option<String> {
    if message.size_bytes <= 0 || message.size_bytes > SNIPPET_MAX_BYTES {
        return None;
    }
    // The blob key came from an already-authorized mailbox read, so this is
    // not a second authorization decision. The disk read *and* the MIME parse
    // run on the blocking pool: the inbox builds up to INBOX_LIMIT snippets per
    // request, and a 256 KB parse on a runtime worker would starve every other
    // request sharing that thread. A panicked/cancelled task degrades to "no
    // preview" rather than failing the page.
    let store = store.clone();
    let blob_key = message.blob_key.clone();
    tokio::task::spawn_blocking(move || {
        let bytes = store.raw_message(&blob_key).ok()?;
        let parsed = tira_smtp::message::parse(&bytes).ok()?;
        let body = parsed.body_text?;
        let snippet = format::snippet(&body, 140);
        (!snippet.is_empty()).then_some(snippet)
    })
    .await
    .ok()
    .flatten()
}

async fn message_model(store: &PgStore, message: &StoredMessage, locale: Locale) -> MessageModel {
    MessageModel {
        id: message.id.to_string(),
        from_addr: message.from_addr.clone(),
        subject: message.subject.clone(),
        snippet: snippet_for(store, message).await,
        size_bytes: message.size_bytes,
        received_label: format::stamp(
            locale,
            message.received_at.with_timezone(&Local).fixed_offset(),
            Local::now().fixed_offset(),
        ),
    }
}

/// `GET /` — a valid mailbox-scoped session goes straight to its inbox; a
/// tenant-wide token gets the honest scope page; everyone else gets the login
/// form. With no database configured the form still renders (200): the failure
/// belongs on submit, not on a page that needs no storage.
pub async fn root(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let locale = locale::resolve(&headers, None);
    let Some(store) = state.store.as_ref() else {
        return html(StatusCode::OK, login_markup(locale, None));
    };

    let Some(secret) = secret_from_headers(&headers) else {
        return html(StatusCode::OK, login_markup(locale, None));
    };

    match store.resolve_token(secret).await {
        Ok(Some(token)) => match token.mailbox_id {
            Some(id) => Redirect::to(&format!("/mail/{id}")).into_response(),
            None => html(StatusCode::OK, pages::scope_notice(locale)),
        },
        Ok(None) => html(
            StatusCode::OK,
            login_markup(locale, Some(locale.auth().err_session_expired)),
        ),
        Err(err) => unavailable(locale, err),
    }
}

/// `GET /login`
///
/// `?lang=` is the switcher link. When it names a supported language, the
/// answer also stores it in a cookie so the following POST renders in the same
/// language (a form submission carries no query string).
pub async fn login_form(Query(query): Query<LangQuery>, headers: HeaderMap) -> Response {
    let locale = locale::resolve(&headers, query.lang.as_deref());
    let mut response = html(StatusCode::OK, login_markup(locale, None));
    if let Some(chosen) = query.lang.as_deref().and_then(Locale::from_code) {
        locale::set_cookie(&mut response, chosen, cookie_secure());
    }
    response
}

/// The login screen gains its "create a mailbox" link only when the dev signup
/// route is actually mounted; otherwise that link would be a dead control.
fn login_markup(locale: Locale, error: Option<&str>) -> tira_frontend::Markup {
    if crate::signup::enabled() {
        pages::login_with_signup(locale, error)
    } else {
        pages::login(locale, error)
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    /// `password` (default) or `token`. The field is optional so a token-only
    /// post from an older client still works and so a malformed form gets an
    /// Arabic explanation instead of a serde error page.
    #[serde(default)]
    mode: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginMode {
    Password,
    Token,
}

/// `POST /login` — two doors, one session.
///
/// * **Password** is the default: email + local Tira password, verified with
///   Argon2id. This is the credential a person can actually remember.
/// * **Token** stays as the bootstrap/recovery path: an operator-issued token
///   can open the profile, where a password is then set.
///
/// Neither secret is logged, echoed into the page, or placed in a URL. Wrong
/// password, unknown address, revoked token and expired token share one
/// generic answer where that is safe to do.
pub async fn login_submit(
    State(state): State<AppState>,
    meta: RequestMeta,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let locale = locale::resolve(&headers, None);
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };

    let mode = if form.mode.eq_ignore_ascii_case("token")
        || (form.email.trim().is_empty() && !form.token.trim().is_empty())
    {
        LoginMode::Token
    } else {
        LoginMode::Password
    };

    match mode {
        LoginMode::Token => token_login(store, &form, &meta, locale).await,
        LoginMode::Password => password_login(&state, store, &form, &meta, locale).await,
    }
}

async fn token_login(
    store: &PgStore,
    form: &LoginForm,
    meta: &RequestMeta,
    locale: Locale,
) -> Response {
    let text = locale.auth();
    let secret = form.token.trim();
    if secret.is_empty() {
        return html(
            StatusCode::BAD_REQUEST,
            pages::login(locale, Some(text.err_token_required)),
        );
    }

    match store.resolve_token(secret).await {
        Ok(Some(token)) => {
            let audit = audit_context(meta, format!("token:{}", token.id));
            let object = token.mailbox_id.map(|id| format!("mailbox:{id}"));
            if let Err(err) = store
                .record_auth_event(
                    Some(token.tenant_id),
                    &audit,
                    "auth.login",
                    object.as_deref(),
                    "ok",
                    serde_json::json!({ "method": "token" }),
                )
                .await
            {
                return unavailable(locale, err);
            }

            let mut response = match token.mailbox_id {
                Some(mailbox_id) => Redirect::to(&format!("/mail/{mailbox_id}")).into_response(),
                None => Redirect::to("/").into_response(),
            };
            set_session_cookie(&mut response, secret);
            response
        }
        Ok(None) => html(
            StatusCode::UNAUTHORIZED,
            pages::login(locale, Some(text.err_token_invalid)),
        ),
        Err(err) => unavailable(locale, err),
    }
}

async fn password_login(
    state: &AppState,
    store: &PgStore,
    form: &LoginForm,
    meta: &RequestMeta,
    locale: Locale,
) -> Response {
    let text = locale.auth();
    let Some(email) = normalize_email(&form.email) else {
        return html(
            StatusCode::BAD_REQUEST,
            pages::login(locale, Some(text.err_bad_email)),
        );
    };

    let password = form.password.as_str();
    if password.is_empty() {
        return html(
            StatusCode::BAD_REQUEST,
            pages::login(locale, Some(text.err_password_required)),
        );
    }
    if password.chars().count() > MAX_PASSWORD_CHARS {
        return html(
            StatusCode::BAD_REQUEST,
            pages::login(locale, Some(text.err_password_long)),
        );
    }

    let limiter_key = login_limiter_key(&email, meta);
    let ip_key = login_ip_limiter_key(meta);
    // Reserve before any Argon2 work, against both buckets, atomically. The
    // coarse per-IP bucket is tested first so a spraying source is refused
    // without touching the per-account ledger; its key omits the address, so it
    // bounds aggregate attempts independent of whether the account exists.
    // `try_attempt*` increments under the same lock as the ceiling test, which
    // closes the old check-then-act race (parallel requests all passing a stale
    // `is_blocked` before any `record_failure` landed).
    if !state
        .login_limiter
        .try_attempt_capped(&ip_key, crate::limiter::DEFAULT_IP_MAX_ATTEMPTS)
        || !state.login_limiter.try_attempt(&limiter_key)
    {
        // The audit row is best-effort here: the answer is already "slow down",
        // and failing it would turn a throttle into a storage error.
        let audit = audit_context(meta, "auth:anonymous".to_string());
        if let Err(err) = store
            .record_auth_event(
                None,
                &audit,
                "auth.login",
                None,
                "denied",
                serde_json::json!({ "method": "password", "reason": "rate_limited" }),
            )
            .await
        {
            tracing::warn!(%err, "could not audit a rate-limited login");
        }
        return too_many_requests(locale);
    }

    let credential = match store.credential_by_address(&email).await {
        Ok(value) => value,
        Err(err) => return unavailable(locale, err),
    };

    // A missing credential still pays for one Argon2id verification against a
    // dummy hash, so response time does not tell an attacker whether the
    // address exists.
    let (tenant_id, mailbox_id, stored_hash, known) = match &credential {
        Some(credential) => (
            Some(credential.tenant_id),
            Some(credential.mailbox_id),
            credential.password_hash.clone(),
            true,
        ),
        None => (None, None, dummy_password_hash().to_string(), false),
    };

    let verified = verify_password_async(password.to_string(), stored_hash).await;

    if !known || !verified {
        // The attempt was reserved (counted) before the verify, so a wrong
        // password or an unknown address simply leaves it counted — there is no
        // second increment to race with.

        let actor = mailbox_id
            .map(|id| format!("mailbox:{id}"))
            .unwrap_or_else(|| "auth:anonymous".to_string());
        let audit = audit_context(meta, actor);
        let object = mailbox_id.map(|id| format!("mailbox:{id}"));
        if let Err(err) = store
            .record_auth_event(
                tenant_id,
                &audit,
                "auth.login",
                object.as_deref(),
                "denied",
                serde_json::json!({ "method": "password" }),
            )
            .await
        {
            return unavailable(locale, err);
        }

        return html(
            StatusCode::UNAUTHORIZED,
            pages::login(locale, Some(text.err_credentials)),
        );
    }

    let (Some(tenant_id), Some(mailbox_id)) = (tenant_id, mailbox_id) else {
        // Defensive: a credential row always carries both. If it did not, the
        // safe answer is the same generic rejection, not a panic.
        return html(
            StatusCode::UNAUTHORIZED,
            pages::login(locale, Some(text.err_credentials)),
        );
    };

    let issued = match store
        .issue_token(
            tenant_id,
            Some(mailbox_id),
            SESSION_LABEL,
            Some(Duration::days(SESSION_TTL_DAYS)),
        )
        .await
    {
        Ok(issued) => issued,
        Err(err) => return unavailable(locale, err),
    };

    let object = format!("mailbox:{mailbox_id}");
    let audit = audit_context(meta, object.clone());
    if let Err(err) = store
        .record_auth_event(
            Some(tenant_id),
            &audit,
            "auth.login",
            Some(object.as_str()),
            "ok",
            serde_json::json!({ "method": "password" }),
        )
        .await
    {
        // A session without an audit row must not exist. Revoke before failing.
        let _ = store.revoke_token(issued.record.id).await;
        return unavailable(locale, err);
    }

    state.login_limiter.clear(&limiter_key);

    let mut response = Redirect::to(&format!("/mail/{mailbox_id}")).into_response();
    set_session_cookie(&mut response, &issued.secret);
    response
}

/// `POST /logout` — revoke the presented session, then clear the cookie.
///
/// Clearing the cookie alone left the `api_tokens` row alive for its full 7-day
/// TTL: a secret captured before logout (proxy log, shared machine, an XSS
/// elsewhere) kept working while the UI implied the session had ended. Revoking
/// first makes logout mean what it says. It stays idempotent and never loops
/// through the form: with no store, no secret, or an already-dead token it still
/// clears the cookie and redirects to `/login`.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    meta: RequestMeta,
) -> Response {
    if let (Some(store), Some(secret)) = (state.store.as_ref(), secret_from_headers(&headers)) {
        if let Ok(Some(token)) = store.resolve_token(secret).await {
            match store.revoke_token(token.id).await {
                Ok(_revoked) => {
                    let actor = token
                        .mailbox_id
                        .map(|id| format!("mailbox:{id}"))
                        .unwrap_or_else(|| "auth:anonymous".to_string());
                    let audit = audit_context(&meta, actor);
                    if let Err(err) = store
                        .record_auth_event(
                            Some(token.tenant_id),
                            &audit,
                            "auth.logout",
                            None,
                            "ok",
                            serde_json::json!({}),
                        )
                        .await
                    {
                        // The token is already revoked; a missing audit row must
                        // not turn a successful logout into a storage error.
                        tracing::warn!(%err, "could not audit logout");
                    }
                }
                Err(err) => tracing::warn!(%err, "could not revoke session on logout"),
            }
        }
    }
    clear_session_redirect("/login")
}

/// Whether the session cookie may ride plain HTTP. **Secure by default**: a
/// 7-day mailbox credential must never be sent over an unencrypted request. A
/// local plain-HTTP demo opts out explicitly with `TIRA_COOKIE_INSECURE=1`;
/// production needs no variable at all. Kept separate from the builders so they
/// stay pure and deterministically testable.
pub(crate) fn cookie_secure() -> bool {
    std::env::var("TIRA_COOKIE_INSECURE").as_deref() != Ok("1")
}

/// Session cookie. `Secure` unless the operator explicitly opted into plain
/// HTTP for a local demo. `__Host-` is deliberately not used: it would forbid
/// the dev opt-out and force a name change across the auth extractor.
fn session_cookie(secret: &str, secure: bool) -> String {
    format!(
        "tira_token={secret}; Path=/; HttpOnly; SameSite=Strict; Max-Age=604800{}",
        if secure { "; Secure" } else { "" }
    )
}

pub(crate) fn set_session_cookie(response: &mut Response, secret: &str) {
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(secret, cookie_secure()))
            .expect("token secrets are ascii"),
    );
}

/// The deletion cookie must carry the same attributes as the one it clears, or
/// a browser keeps the original. `Max-Age=0` expires it immediately.
fn clearing_cookie(secure: bool) -> String {
    format!(
        "tira_token=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

fn clear_session_redirect(location: &str) -> Response {
    let mut response = Redirect::to(location).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clearing_cookie(cookie_secure()))
            .expect("cookie attributes are ascii"),
    );
    response
}

pub(crate) fn too_many_requests(locale: Locale) -> Response {
    let mut response = html(
        StatusCode::TOO_MANY_REQUESTS,
        pages::login(locale, Some(locale.auth().err_too_many)),
    );
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("600"));
    response
}

pub(crate) fn audit_context(meta: &RequestMeta, actor: String) -> AuditContext {
    AuditContext {
        request_id: meta.request_id,
        actor,
        ip: meta.ip,
        user_agent: meta.user_agent.clone(),
    }
}

/// Lowercase, trim, and reject obvious non-addresses. The schema stores
/// addresses lowercase, so this is the same normalisation the store expects.
fn normalize_email(raw: &str) -> Option<String> {
    let email = raw.trim().to_ascii_lowercase();
    if email.len() < MIN_LOGIN_EMAIL_CHARS || email.len() > MAX_LOGIN_EMAIL_CHARS {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return None;
    }
    if email
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }
    Some(email)
}

fn login_limiter_key(email: &str, meta: &RequestMeta) -> String {
    let ip = meta
        .ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!("login:{email}|{ip}")
}

/// Coarse per-IP login bucket key. Deliberately omits the address so it bounds
/// aggregate attempts from one source regardless of how many distinct — or
/// non-existent — addresses it tries. Reserved with `DEFAULT_IP_MAX_ATTEMPTS`
/// and never refunded by a success, so interleaving one good login cannot reset
/// a spray counter.
fn login_ip_limiter_key(meta: &RequestMeta) -> String {
    let ip = meta
        .ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!("login-ip:{ip}")
}

/// One dummy Argon2id hash, generated once per process. It exists only to make
/// "unknown address" cost the same as "wrong password".
fn dummy_password_hash() -> &'static str {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY.get_or_init(|| {
        tira_auth::password::hash_password("tira-timing-equalizer-not-a-secret")
            .expect("Argon2id with default parameters cannot fail")
    })
}

async fn verify_password_async(password: String, stored: String) -> bool {
    tokio::task::spawn_blocking(move || tira_auth::password::verify_password(&password, &stored))
        .await
        .unwrap_or(false)
}

pub(crate) async fn hash_password_async(password: String) -> Result<String, ()> {
    tokio::task::spawn_blocking(move || tira_auth::password::hash_password(&password))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

/// `GET /mail/{mailbox_id}`
pub async fn inbox_page(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Path(mailbox_id): Path<String>,
    Query(query): Query<InboxQuery>,
) -> Response {
    let locale = locale::resolve(&headers, query.lang.as_deref());
    if state.store.is_none() {
        return no_store(locale);
    }
    let Ok(mailbox_id) = Uuid::parse_str(&mailbox_id) else {
        return problem(StatusCode::BAD_REQUEST, locale, ProblemKind::BadLink);
    };

    // A confirmation is rendered only when the id was issued by this process
    // for this same mailbox. Anything else is silently ignored, never invented.
    let sent = query
        .sent
        .as_deref()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .and_then(|id| state.sent_notices.get(mailbox_id, id));

    let response = match inbox_model_for(&state, &auth, &meta, mailbox_id, locale).await {
        Ok((inbox, _page)) => html(
            StatusCode::OK,
            pages::inbox_with_notice(locale, &inbox, sent.as_ref()),
        ),
        Err(err) => err.into_response(locale),
    };
    persist_locale(response, query.lang.as_deref())
}

/// Query string of the inbox route. `sent` is the id of a one-shot SMTP
/// confirmation issued by `POST /mail/{id}/send`; `lang` is the language
/// switcher's relative link.
#[derive(Debug, Default, Deserialize)]
pub struct InboxQuery {
    #[serde(default)]
    pub sent: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
}

/// Loads the list half of the workspace, plus the page it was built from.
///
/// The message route needs both: the page authorizes the request and carries
/// the mailbox facts, and the model fills the list pane. Returning the page
/// means opening a message does not run the same inbox query twice.
async fn inbox_model_for(
    state: &AppState,
    auth: &PageAuth,
    meta: &RequestMeta,
    mailbox_id: Uuid,
    locale: Locale,
) -> Result<(InboxModel, tira_store::MailboxPage), ReadingError> {
    let Some(store) = state.store.as_ref() else {
        return Err(ReadingError::NoStore);
    };

    let page = match store
        .read_mailbox_page(
            mailbox_id,
            auth.0.tenant_id,
            auth.0.mailbox_id,
            INBOX_LIMIT,
            0,
            &meta.audit(auth.0.token_id),
        )
        .await
    {
        Ok(Some(page)) => page,
        Ok(None) => return Err(ReadingError::MissingMailbox),
        Err(err) => return Err(ReadingError::Unavailable(err.to_string())),
    };

    let mailbox = mailbox_model(&page);
    // Sequential on purpose: each snippet is its own blocking-pool task, so the
    // runtime worker is never parked, and one request cannot fan out enough
    // concurrent disk reads to saturate the blocking pool.
    let mut messages = Vec::with_capacity(page.messages.len());
    for message in &page.messages {
        messages.push(message_model(store, message, locale).await);
    }

    Ok((
        InboxModel {
            mailbox,
            messages,
            server_time: server_time(),
        },
        page,
    ))
}

/// Why a reading view could not be built. Each variant renders the exact page
/// the old inline handler returned, so the screen and print routes cannot
/// drift apart. The enum stays small enough to live in a `Result` without
/// boxing a whole `Response` on the happy path.
enum ReadingError {
    NoStore,
    BadId,
    MissingMailbox,
    MissingMessage,
    WrongMailbox,
    Unavailable(String),
}

impl ReadingError {
    fn into_response(self, locale: Locale) -> Response {
        match self {
            Self::NoStore => no_store(locale),
            Self::BadId => problem(StatusCode::BAD_REQUEST, locale, ProblemKind::BadMessageLink),
            Self::MissingMailbox => {
                problem(StatusCode::NOT_FOUND, locale, ProblemKind::MissingMailbox)
            }
            Self::MissingMessage => {
                problem(StatusCode::NOT_FOUND, locale, ProblemKind::MissingMessage)
            }
            Self::WrongMailbox => problem(StatusCode::NOT_FOUND, locale, ProblemKind::WrongMailbox),
            Self::Unavailable(err) => unavailable(locale, err),
        }
    }
}

/// Loads the reading model shared by the screen page and the print page.
///
/// This keeps RLS, token scope, the sibling-mailbox check, and the
/// `message.read` audit row identical on both surfaces.
async fn reading_model(
    state: &AppState,
    auth: &PageAuth,
    meta: &RequestMeta,
    mailbox_id: &str,
    message_id: &str,
    locale: Locale,
) -> Result<ReadingModel, ReadingError> {
    let Some(store) = state.store.as_ref() else {
        return Err(ReadingError::NoStore);
    };

    let (Ok(mailbox_id), Ok(message_id)) =
        (Uuid::parse_str(mailbox_id), Uuid::parse_str(message_id))
    else {
        return Err(ReadingError::BadId);
    };

    let audit = meta.audit(auth.0.token_id);

    // Authorize the mailbox (also gives us the address and quota for the shell).
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
        Ok(None) => return Err(ReadingError::MissingMailbox),
        Err(err) => return Err(ReadingError::Unavailable(err.to_string())),
    };

    reading_model_from_page(state, auth, meta, &page, message_id, locale).await
}

/// The rest of the reading model, once the mailbox page has authorized the
/// request. Split out so the split-view route can reuse the inbox page it has
/// already loaded instead of reading the same rows twice.
async fn reading_model_from_page(
    state: &AppState,
    auth: &PageAuth,
    meta: &RequestMeta,
    page: &tira_store::MailboxPage,
    message_id: Uuid,
    locale: Locale,
) -> Result<ReadingModel, ReadingError> {
    let Some(store) = state.store.as_ref() else {
        return Err(ReadingError::NoStore);
    };
    let mailbox_id = page.mailbox.id;
    let audit = meta.audit(auth.0.token_id);

    // Authorize and fetch the bytes. This writes `message.read` to the audit
    // trail inside the RLS transaction.
    let bytes = match store
        .read_message_bytes(message_id, auth.0.tenant_id, auth.0.mailbox_id, &audit)
        .await
    {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Err(ReadingError::MissingMessage),
        Err(err) => return Err(ReadingError::Unavailable(err.to_string())),
    };

    // The bytes are authorized, but the URL also names a mailbox. Refuse a
    // message that belongs to a sibling mailbox instead of rendering a
    // mismatched shell.
    let stored = match store
        .get_message_scoped(message_id, auth.0.tenant_id, auth.0.mailbox_id)
        .await
    {
        Ok(Some(stored)) => stored,
        Ok(None) => return Err(ReadingError::MissingMessage),
        Err(err) => return Err(ReadingError::Unavailable(err.to_string())),
    };
    if stored.mailbox_id != mailbox_id {
        return Err(ReadingError::WrongMailbox);
    }

    // Body only: headers come from the stored row, which is authoritative and
    // already decoded. A parse failure is not fatal — the page says so. The
    // parse is CPU-bound and the message has no snippet size cap here, so it
    // runs on the blocking pool; a panicked/cancelled task yields "no body".
    let body = tokio::task::spawn_blocking(move || {
        tira_smtp::message::parse(&bytes)
            .ok()
            .and_then(|parsed| parsed.body_text)
    })
    .await
    .ok()
    .flatten();

    // Mail-authentication verdicts for the sender domain, straight from the
    // same live evidence the control-plane proof re-derives. Unavailable means
    // "not shown here", never "passed".
    let from_domain = format::domain_part(&stored.from_addr).to_string();
    let authenticity = crate::proof::mail_authentication_for(state, &from_domain)
        .await
        .map(|(mail, checks)| tira_frontend::MessageAuth {
            domain: mail.domain,
            checks: checks
                .into_iter()
                .map(|check| tira_frontend::CheckView {
                    id: check.id,
                    status: check.status,
                    detail: check.detail,
                })
                .collect(),
        });

    Ok(ReadingModel {
        mailbox: mailbox_model(page),
        server_time: server_time(),
        message: ReadingMessage {
            id: stored.id.to_string(),
            subject: stored.subject.clone(),
            from_addr: stored.from_addr.clone(),
            to_addr: page.mailbox.address.clone(),
            message_id_hdr: stored.message_id_hdr.clone(),
            size_bytes: stored.size_bytes,
            received_full: format::full_stamp(
                locale,
                stored.received_at.with_timezone(&Local).fixed_offset(),
            ),
            received_iso: stored.received_at.to_rfc3339(),
            body,
            authenticity,
        },
    })
}

/// `GET /mail/{mailbox_id}/message/{message_id}`
///
/// The split workspace: the list and the reader are one response, so opening a
/// message never loses the list it came from. The inbox page loaded here is
/// also the authorization check, so this route costs one inbox query, not two.
pub async fn message_page(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Path((mailbox_id, message_id)): Path<(String, String)>,
    Query(query): Query<LangQuery>,
) -> Response {
    let locale = locale::resolve(&headers, query.lang.as_deref());
    if state.store.is_none() {
        return no_store(locale);
    }
    let Ok(mailbox_id) = Uuid::parse_str(&mailbox_id) else {
        return ReadingError::BadId.into_response(locale);
    };
    let Ok(message_id) = Uuid::parse_str(&message_id) else {
        return ReadingError::BadId.into_response(locale);
    };

    let response = match inbox_model_for(&state, &auth, &meta, mailbox_id, locale).await {
        Ok((inbox, page)) => {
            match reading_model_from_page(&state, &auth, &meta, &page, message_id, locale).await {
                Ok(reading) => html(
                    StatusCode::OK,
                    pages::inbox_with_reader(locale, &inbox, &reading),
                ),
                Err(err) => err.into_response(locale),
            }
        }
        Err(err) => err.into_response(locale),
    };
    persist_locale(response, query.lang.as_deref())
}

/// `GET /mail/{mailbox_id}/message/{message_id}/print`
///
/// A separate navigation target so printing works without JavaScript. The
/// loader is the same one the reading page uses; this route cannot widen
/// access to a message.
pub async fn message_print_page(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Path((mailbox_id, message_id)): Path<(String, String)>,
    Query(query): Query<LangQuery>,
) -> Response {
    let locale = locale::resolve(&headers, query.lang.as_deref());
    let response = match reading_model(&state, &auth, &meta, &mailbox_id, &message_id, locale).await
    {
        Ok(model) => html(StatusCode::OK, pages::message_print(locale, &model)),
        Err(err) => err.into_response(locale),
    };
    persist_locale(response, query.lang.as_deref())
}

/// Query string for the compose screen. Only the reply prefill fields are
/// read; they are not secrets, and an unknown key is ignored rather than
/// reflected.
#[derive(Debug, Default, Deserialize)]
pub struct ComposeQuery {
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
}

/// `GET /mail/{mailbox_id}/compose` — the reply/compose screen.
///
/// The reply recipient and subject are prefill inputs produced by the reader;
/// the body starts empty. The mailbox must be the caller's own, exactly like
/// every other mailbox-scoped screen.
pub async fn compose_page(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Path(mailbox_id): Path<String>,
    Query(query): Query<ComposeQuery>,
) -> Response {
    let locale = locale::resolve(&headers, query.lang.as_deref());
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };
    let Ok(mailbox_id) = Uuid::parse_str(&mailbox_id) else {
        return problem(
            StatusCode::BAD_REQUEST,
            locale,
            ProblemKind::BadComposeRequest,
        );
    };
    if auth.0.mailbox_id != Some(mailbox_id) {
        return problem(StatusCode::NOT_FOUND, locale, ProblemKind::MissingMailbox);
    }

    let page = match store
        .read_mailbox_page(
            mailbox_id,
            auth.0.tenant_id,
            auth.0.mailbox_id,
            1,
            0,
            &meta.audit(auth.0.token_id),
        )
        .await
    {
        Ok(Some(page)) => page,
        Ok(None) => {
            return problem(StatusCode::NOT_FOUND, locale, ProblemKind::MissingMailbox);
        }
        Err(err) => return unavailable(locale, err),
    };

    let to = query
        .to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_string();
    let subject = query
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_string();

    let response = html(
        StatusCode::OK,
        pages::compose(locale, &mailbox_model(&page), &to, &subject, "", None),
    );
    persist_locale(response, query.lang.as_deref())
}

/// Query string for the profile page. Only fixed, known values are turned into
/// text; an arbitrary `?status=` or `?error=` is ignored rather than reflected.
#[derive(Debug, Default, Deserialize)]
pub struct ProfileQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    lang: Option<String>,
}

/// `GET /profile`
///
/// The page is scoped to exactly one mailbox. A tenant-wide token cannot pick
/// one, so it is sent back to `/`, which already explains that limitation.
pub async fn profile_page(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Query(query): Query<ProfileQuery>,
) -> Response {
    let locale = locale::resolve(&headers, query.lang.as_deref());
    let response = render_profile(&state, &auth, &meta, &query, locale).await;
    persist_locale(response, query.lang.as_deref())
}

async fn render_profile(
    state: &AppState,
    auth: &PageAuth,
    _meta: &RequestMeta,
    query: &ProfileQuery,
    locale: Locale,
) -> Response {
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };
    let Some(mailbox_id) = auth.0.mailbox_id else {
        return Redirect::to("/").into_response();
    };
    let tenant_id = auth.0.tenant_id;

    let profile = match store.mailbox_profile(mailbox_id, tenant_id).await {
        Ok(Some(profile)) => profile,
        Ok(None) => return problem(StatusCode::NOT_FOUND, locale, ProblemKind::MissingMailbox),
        Err(err) => return unavailable(locale, err),
    };

    let sessions = match store.list_mailbox_sessions(mailbox_id, tenant_id).await {
        Ok(sessions) => sessions,
        Err(err) => return unavailable(locale, err),
    };

    let (status, error) = profile_notice(locale, query);
    let text = locale.shell();

    let model = ProfileModel {
        mailbox: mailbox_model_from_profile(&profile),
        display_name: profile.display_name.clone(),
        address: profile.address.clone(),
        created_label: format::full_stamp(
            locale,
            profile.created_at.with_timezone(&Local).fixed_offset(),
        ),
        message_count: profile.message_count,
        password_set: profile.password_set,
        engine_state: profile.engine_state.clone(),
        sessions: sessions
            .iter()
            .map(|session| SessionModel {
                id: session.id.to_string(),
                label: session.label.clone(),
                created_label: format::full_stamp(
                    locale,
                    session.created_at.with_timezone(&Local).fixed_offset(),
                ),
                last_used_label: session
                    .last_used_at
                    .map(|at| format::full_stamp(locale, at.with_timezone(&Local).fixed_offset()))
                    .unwrap_or_else(|| text.never_used.to_string()),
                expires_label: session
                    .expires_at
                    .map(|at| format::full_stamp(locale, at.with_timezone(&Local).fixed_offset()))
                    .unwrap_or_else(|| text.never_expires.to_string()),
                is_current: session.id == auth.0.token_id,
            })
            .collect(),
        current_token_id: auth.0.token_id.to_string(),
        status,
        error,
    };

    html(StatusCode::OK, pages::profile(locale, &model))
}

fn mailbox_model_from_profile(profile: &MailboxProfile) -> MailboxModel {
    MailboxModel {
        id: profile.id.to_string(),
        address: profile.address.clone(),
        display_name: profile.display_name.clone(),
        total: profile.message_count,
        used_bytes: profile.used_bytes,
        quota_bytes: profile.quota_bytes,
    }
}

fn profile_notice(
    locale: Locale,
    query: &ProfileQuery,
) -> (Option<&'static str>, Option<&'static str>) {
    let text = locale.shell();
    let status = match query.status.as_deref() {
        Some("name-saved") => Some(text.status_name_saved),
        Some("password-set") => Some(text.status_password_set),
        Some("password-changed") => Some(text.status_password_changed),
        Some("session-revoked") => Some(text.status_session_revoked),
        Some("others-revoked") => Some(text.status_others_revoked),
        Some("no-other-sessions") => Some(text.status_no_other_sessions),
        _ => None,
    };

    let error = match query.error.as_deref() {
        Some("name-too-long") => Some(text.err_name_too_long),
        Some("password-short") => Some(text.err_password_short),
        Some("password-long") => Some(text.err_password_long),
        Some("password-mismatch") => Some(text.err_password_mismatch),
        Some("password-same") => Some(text.err_password_same),
        Some("password-equals-email") => Some(text.err_password_equals_email),
        Some("current-required") => Some(text.err_current_required),
        Some("current-wrong") => Some(text.err_current_wrong),
        Some("rate-limited") => Some(text.err_rate_limited),
        Some("session-not-found") => Some(text.err_session_not_found),
        Some("hash-failed") => Some(text.err_hash_failed),
        _ => None,
    };

    (status, error)
}

fn redirect_profile_status(status: &str) -> Response {
    Redirect::to(&format!("/profile?status={status}")).into_response()
}

fn redirect_profile_error(error: &str) -> Response {
    Redirect::to(&format!("/profile?error={error}")).into_response()
}

#[derive(Debug, Deserialize)]
pub struct DisplayNameForm {
    #[serde(default)]
    display_name: String,
}

/// `POST /profile/display-name`
pub async fn profile_display_name(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Form(form): Form<DisplayNameForm>,
) -> Response {
    let locale = locale::resolve(&headers, None);
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };
    let Some(mailbox_id) = auth.0.mailbox_id else {
        return Redirect::to("/").into_response();
    };
    let tenant_id = auth.0.tenant_id;

    let trimmed = form.display_name.trim();
    if trimmed.chars().count() > MAX_DISPLAY_NAME_CHARS {
        return redirect_profile_error("name-too-long");
    }
    let value = (!trimmed.is_empty()).then_some(trimmed);

    match store.set_display_name(mailbox_id, tenant_id, value).await {
        Ok(true) => {}
        Ok(false) => return problem(StatusCode::NOT_FOUND, locale, ProblemKind::DeletedMailbox),
        Err(err) => return unavailable(locale, err),
    }

    let object = format!("mailbox:{mailbox_id}");
    let audit = audit_context(&meta, object.clone());
    if let Err(err) = store
        .record_auth_event(
            Some(tenant_id),
            &audit,
            "profile.display_name",
            Some(object.as_str()),
            "ok",
            serde_json::json!({ "set": value.is_some() }),
        )
        .await
    {
        return unavailable(locale, err);
    }

    redirect_profile_status("name-saved")
}

#[derive(Debug, Deserialize)]
pub struct PasswordForm {
    #[serde(default)]
    current_password: String,
    #[serde(default)]
    new_password: String,
    #[serde(default)]
    confirm_password: String,
}

/// `POST /profile/password`
///
/// Sets the local Tira credential. This is explicitly **not** a Stalwart/IMAP
/// password: the mail engine is a separate account, and the profile says so.
/// A successful change revokes every other mailbox-scoped session, so a stolen
/// session cannot survive the password rotation.
pub async fn profile_password(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Form(form): Form<PasswordForm>,
) -> Response {
    let locale = locale::resolve(&headers, None);
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };
    let Some(mailbox_id) = auth.0.mailbox_id else {
        return Redirect::to("/").into_response();
    };
    let tenant_id = auth.0.tenant_id;

    let ip = meta
        .ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let limiter_key = format!("profile-password:{mailbox_id}|{ip}");
    // Cheap read-only early-out for an already-full bucket. The authoritative
    // atomic reserve happens just before the current-password verify below, so
    // this stale check can only refuse early — never grant an extra guess.
    if state.login_limiter.is_blocked(&limiter_key) {
        return redirect_profile_error("rate-limited");
    }

    let profile = match store.mailbox_profile(mailbox_id, tenant_id).await {
        Ok(Some(profile)) => profile,
        Ok(None) => return problem(StatusCode::NOT_FOUND, locale, ProblemKind::DeletedMailbox),
        Err(err) => return unavailable(locale, err),
    };

    // When a credential already exists, the current password must be proven.
    // This stops a borrowed session from silently replacing the password.
    let mut stored_hash = None;
    if profile.password_set {
        if form.current_password.is_empty() {
            return redirect_profile_error("current-required");
        }
        let hash = match store.password_hash(mailbox_id, tenant_id).await {
            Ok(Some(hash)) => hash,
            Ok(None) => return redirect_profile_error("current-required"),
            Err(err) => return unavailable(locale, err),
        };
        // Reserve atomically before the Argon2 verify, closing the same
        // check-then-act race as login. Only an actual current-password guess
        // is counted; `clear` refunds it on a completed change, so fumbling the
        // *new* password's validation never burns a slot.
        if !state.login_limiter.try_attempt(&limiter_key) {
            return redirect_profile_error("rate-limited");
        }
        if !verify_password_async(form.current_password.clone(), hash.clone()).await {
            let object = format!("mailbox:{mailbox_id}");
            let audit = audit_context(&meta, object.clone());
            if let Err(err) = store
                .record_auth_event(
                    Some(tenant_id),
                    &audit,
                    "profile.password",
                    Some(object.as_str()),
                    "denied",
                    serde_json::json!({ "reason": "current_wrong" }),
                )
                .await
            {
                return unavailable(locale, err);
            }

            return redirect_profile_error("current-wrong");
        }
        stored_hash = Some(hash);
    }

    let new_password = form.new_password.as_str();
    let chars = new_password.chars().count();
    if chars < MIN_PASSWORD_CHARS {
        return redirect_profile_error("password-short");
    }
    if chars > MAX_PASSWORD_CHARS {
        return redirect_profile_error("password-long");
    }
    if new_password != form.confirm_password {
        return redirect_profile_error("password-mismatch");
    }
    if new_password.eq_ignore_ascii_case(&profile.address)
        || new_password.eq_ignore_ascii_case(format::local_part(&profile.address))
    {
        return redirect_profile_error("password-equals-email");
    }
    if let Some(hash) = &stored_hash {
        if verify_password_async(new_password.to_string(), hash.clone()).await {
            return redirect_profile_error("password-same");
        }
    }

    let hash = match hash_password_async(new_password.to_string()).await {
        Ok(hash) => hash,
        Err(()) => return redirect_profile_error("hash-failed"),
    };

    if let Err(err) = store.set_password(mailbox_id, tenant_id, &hash).await {
        return unavailable(locale, err);
    }

    // The current session stays; every other mailbox session is cut. The user
    // asked for a password because tokens are hard to keep — a rotation must
    // also retire the old copies.
    let revoked = match store
        .revoke_other_mailbox_sessions(auth.0.token_id, mailbox_id, tenant_id)
        .await
    {
        Ok(count) => count,
        Err(err) => return unavailable(locale, err),
    };

    let action = if profile.password_set {
        "profile.password.change"
    } else {
        "profile.password.set"
    };
    let object = format!("mailbox:{mailbox_id}");
    let audit = audit_context(&meta, object.clone());
    if let Err(err) = store
        .record_auth_event(
            Some(tenant_id),
            &audit,
            action,
            Some(object.as_str()),
            "ok",
            serde_json::json!({ "revoked_others": revoked }),
        )
        .await
    {
        return unavailable(locale, err);
    }

    state.login_limiter.clear(&limiter_key);
    redirect_profile_status(if profile.password_set {
        "password-changed"
    } else {
        "password-set"
    })
}

#[derive(Debug, Deserialize)]
pub struct RevokeSessionForm {
    #[serde(default)]
    session_id: String,
}

/// `POST /profile/sessions/revoke`
pub async fn profile_revoke_session(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
    Form(form): Form<RevokeSessionForm>,
) -> Response {
    let locale = locale::resolve(&headers, None);
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };
    let Some(mailbox_id) = auth.0.mailbox_id else {
        return Redirect::to("/").into_response();
    };
    let tenant_id = auth.0.tenant_id;

    let Ok(session_id) = Uuid::parse_str(form.session_id.trim()) else {
        return redirect_profile_error("session-not-found");
    };

    let revoked = match store
        .revoke_mailbox_session(session_id, mailbox_id, tenant_id)
        .await
    {
        Ok(revoked) => revoked,
        Err(err) => return unavailable(locale, err),
    };

    let object = format!("session:{session_id}");
    let audit = audit_context(&meta, format!("mailbox:{mailbox_id}"));
    if let Err(err) = store
        .record_auth_event(
            Some(tenant_id),
            &audit,
            "profile.session.revoke",
            Some(object.as_str()),
            if revoked { "ok" } else { "denied" },
            serde_json::json!({}),
        )
        .await
    {
        return unavailable(locale, err);
    }

    if !revoked {
        return redirect_profile_error("session-not-found");
    }
    if session_id == auth.0.token_id {
        // Revoking the session you are using is a logout, not an error.
        return clear_session_redirect("/login");
    }
    redirect_profile_status("session-revoked")
}

/// `POST /profile/sessions/revoke-others`
pub async fn profile_revoke_others(
    State(state): State<AppState>,
    auth: PageAuth,
    meta: RequestMeta,
    headers: HeaderMap,
) -> Response {
    let locale = locale::resolve(&headers, None);
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };
    let Some(mailbox_id) = auth.0.mailbox_id else {
        return Redirect::to("/").into_response();
    };
    let tenant_id = auth.0.tenant_id;

    let revoked = match store
        .revoke_other_mailbox_sessions(auth.0.token_id, mailbox_id, tenant_id)
        .await
    {
        Ok(count) => count,
        Err(err) => return unavailable(locale, err),
    };

    let object = format!("mailbox:{mailbox_id}");
    let audit = audit_context(&meta, object.clone());
    if let Err(err) = store
        .record_auth_event(
            Some(tenant_id),
            &audit,
            "profile.sessions.revoke_others",
            Some(object.as_str()),
            "ok",
            serde_json::json!({ "revoked": revoked }),
        )
        .await
    {
        return unavailable(locale, err);
    }

    if revoked == 0 {
        redirect_profile_status("no-other-sessions")
    } else {
        redirect_profile_status("others-revoked")
    }
}

/// `GET /assets/app.css`
pub async fn asset_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=300"),
            (header::ETAG, tira_frontend::assets::CSS_ETAG.as_str()),
        ],
        tira_frontend::assets::APP_CSS,
    )
        .into_response()
}

/// `GET /assets/fonts/{file}`
///
/// Only the eight embedded files resolve. The name never touches the file
/// system, so `../` and absolute paths are simply unknown names.
pub async fn asset_font(Path(file): Path<String>) -> Response {
    let Some(asset) = tira_frontend::assets::font(&file) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut response = Body::from(asset.bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("font/woff2"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&tira_frontend::assets::font_etag(asset))
            .expect("font etags are ascii"),
    );
    response
}

/// `GET /assets/htmx.min.js`
pub async fn asset_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=86400"),
            (header::ETAG, tira_frontend::assets::HTMX_ETAG),
        ],
        tira_frontend::assets::HTMX_JS,
    )
        .into_response()
}

/// `GET /assets/nav-a11y.js`
///
/// The navigation-accessibility enhancer. Self-hosted like HTMX and served
/// with a content-derived ETag, so an edit busts the cache. `max-age=300`
/// matches the stylesheet: this script is ours and may evolve.
pub async fn asset_nav_a11y() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=300"),
            (header::ETAG, tira_frontend::assets::NAV_A11Y_ETAG.as_str()),
        ],
        tira_frontend::assets::NAV_A11Y_JS,
    )
        .into_response()
}

/// `GET /assets/auth.js`
///
/// The login/signup enhancer. Served only because the auth pages reference it;
/// it is self-hosted like every other asset and carries a content-derived
/// ETag, so an edit busts the cache. `max-age=300` matches the other
/// first-party scripts: this one may evolve.
pub async fn asset_auth_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=300"),
            (header::ETAG, tira_frontend::assets::AUTH_ETAG.as_str()),
        ],
        tira_frontend::assets::AUTH_JS,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_is_secure_and_locked_down() {
        let cookie = session_cookie("tira_abc123", true);
        assert!(cookie.starts_with("tira_token=tira_abc123;"));
        assert!(cookie.contains("; Secure"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Max-Age=604800"));
        // No Domain attribute: the cookie is host-only, not shared with siblings.
        assert!(!cookie.contains("Domain="));
    }

    #[test]
    fn insecure_opt_out_drops_only_the_secure_flag() {
        let cookie = session_cookie("tira_abc123", false);
        assert!(!cookie.contains("Secure"));
        // Every other protection survives the dev opt-out.
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/"));
    }

    #[test]
    fn clearing_cookie_expires_and_matches_attributes() {
        let secure = clearing_cookie(true);
        assert!(secure.starts_with("tira_token=;"));
        assert!(secure.contains("Max-Age=0"));
        assert!(secure.contains("; Secure"));
        assert!(secure.contains("HttpOnly"));
        assert!(secure.contains("SameSite=Strict"));
        assert!(secure.contains("Path=/"));
        assert!(!clearing_cookie(false).contains("Secure"));
    }

    #[test]
    fn rendered_pages_are_never_cacheable() {
        let response = html(StatusCode::OK, pages::login(Locale::Ar, None));
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(response.headers().get(header::VARY).unwrap(), "Cookie");
    }
}
