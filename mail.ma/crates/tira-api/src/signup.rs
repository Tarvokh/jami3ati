//! Local-development signup: a person creates their own mailbox.
//!
//! This route exists only when `TIRA_DEV_MODE=1` **and** the peer is loopback;
//! `app()` does not mount it otherwise, so a normal deployment cannot reach it
//! even by guessing the URL. Public self-service registration is a product
//! decision (abuse handling, quotas, domain ownership), not something a demo
//! flag may switch on by accident.
//!
//! What it creates is real, not a fixture: a `tenants` row, a mailbox, an
//! Argon2id credential, a session token, and an audit entry. The instance starts
//! with an empty inbox, so the first message a person reads is one they sent
//! themselves through the live compose path.
//!
//! Failure shapes are deliberately boring: every validation problem re-renders
//! the form with the values kept and one Arabic sentence. A taken address is
//! the same sentence as any other conflict — the form is loopback-only, so it
//! is not an enumeration oracle worth a separate status code.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::Duration;
use serde::Deserialize;
use tira_frontend::i18n::Locale;
use tira_frontend::pages::SignupValues;
use tira_store::StoreError;

use crate::locale::{self, LangQuery};
use crate::pages::{
    audit_context, cookie_secure, hash_password_async, html, no_store, set_session_cookie,
    too_many_requests, unavailable, MAX_PASSWORD_CHARS, MIN_PASSWORD_CHARS, SESSION_LABEL,
    SESSION_TTL_DAYS,
};
use crate::read::RequestMeta;
use crate::AppState;

/// Same default as the schema (5 GiB); stated once so the number cannot drift
/// between the bootstrap and a signup.
const QUOTA_BYTES: i64 = 5 * 1024 * 1024 * 1024;

const MIN_LOCAL_PART: usize = 2;
const MAX_LOCAL_PART: usize = 32;
const MAX_DISPLAY_NAME: usize = 80;

/// Signups allowed per client address inside the limiter window. A person
/// creating a handful of test accounts is fine; a script is not.
const MAX_SIGNUPS_PER_WINDOW: u32 = 20;

/// `TIRA_DEV_MODE=1` (or `true`) is the only thing that mounts this module.
pub fn enabled() -> bool {
    matches!(
        std::env::var("TIRA_DEV_MODE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("True")
    )
}

/// The domain every locally created mailbox belongs to. The operator creates it
/// once (`scripts/dev-local.ps1`); signup never invents a tenant.
pub fn domain() -> String {
    std::env::var("TIRA_DEV_DOMAIN")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value.contains('.') && !value.contains('@'))
        .unwrap_or_else(|| "tira.test".to_string())
}

/// `GET /signup`
///
/// `?lang=` is the switcher link; it is persisted in a cookie so the following
/// POST — which carries no query string — renders in the same language.
pub async fn form(Query(query): Query<LangQuery>, headers: HeaderMap) -> Response {
    let locale = locale::resolve(&headers, query.lang.as_deref());
    let mut response = html(
        StatusCode::OK,
        tira_frontend::pages::signup(locale, &domain(), &SignupValues::default(), None),
    );
    if let Some(chosen) = query.lang.as_deref().and_then(Locale::from_code) {
        locale::set_cookie(&mut response, chosen, cookie_secure());
    }
    response
}

#[derive(Debug, Deserialize)]
pub struct SignupForm {
    #[serde(default)]
    local: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    confirm: String,
}

/// `POST /signup`
pub async fn submit(
    State(state): State<AppState>,
    meta: RequestMeta,
    headers: HeaderMap,
    Form(form): Form<SignupForm>,
) -> Response {
    let locale = locale::resolve(&headers, None);
    let Some(store) = state.store.as_ref() else {
        return no_store(locale);
    };
    let text = locale.auth();

    let domain = domain();
    let values = SignupValues {
        local: form.local.trim().to_ascii_lowercase(),
        display_name: form.display_name.trim().to_string(),
    };

    let reject = |message: &str| -> Response {
        html(
            StatusCode::BAD_REQUEST,
            tira_frontend::pages::signup(locale, &domain, &values, Some(message)),
        )
    };

    let key = format!(
        "signup:{}",
        meta.ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    if !state
        .login_limiter
        .try_attempt_capped(&key, MAX_SIGNUPS_PER_WINDOW)
    {
        return too_many_requests(locale);
    }

    let Some(local_part) = normalize_local_part(&values.local) else {
        return reject(text.err_local_invalid);
    };

    if values.display_name.chars().count() > MAX_DISPLAY_NAME {
        return reject(text.err_display_long);
    }

    let password_chars = form.password.chars().count();
    if password_chars < MIN_PASSWORD_CHARS {
        return reject(text.err_password_short);
    }
    if password_chars > MAX_PASSWORD_CHARS {
        return reject(text.err_password_too_long);
    }
    if form.password != form.confirm {
        return reject(text.err_mismatch);
    }

    let Some((tenant_id, domain_id)) = (match store.domain_by_name(&domain).await {
        Ok(found) => found,
        Err(err) => return unavailable(locale, err),
    }) else {
        // The operator has not run the bootstrap. Saying so is the only honest
        // answer; creating a tenant here would hide a broken setup.
        return html(
            StatusCode::SERVICE_UNAVAILABLE,
            tira_frontend::pages::problem(
                locale,
                locale.shell().local_domain_eyebrow,
                locale.shell().local_domain_title,
                &locale.local_domain_missing(&domain),
            ),
        );
    };

    let mailbox = match store
        .create_mailbox(domain_id, &local_part, QUOTA_BYTES)
        .await
    {
        Ok(mailbox) => mailbox,
        Err(err) if is_conflict(&err) => {
            return reject(text.err_taken);
        }
        Err(err) => return unavailable(locale, err),
    };

    let password_hash = match hash_password_async(form.password.clone()).await {
        Ok(hash) => hash,
        Err(()) => return unavailable(locale, "password hashing failed"),
    };

    if let Err(err) = store
        .set_password(mailbox.id, tenant_id, &password_hash)
        .await
    {
        // The mailbox row stays, but with no credential and no session it is
        // unreachable. The operator can delete it; retrying the same name hits
        // the unique index, so the form says "already taken" — which is true.
        tracing::warn!(%err, mailbox = %mailbox.id, "signup: credential write failed");
        return unavailable(locale, err);
    }

    if !values.display_name.is_empty() {
        if let Err(err) = store
            .set_display_name(mailbox.id, tenant_id, Some(&values.display_name))
            .await
        {
            tracing::warn!(%err, mailbox = %mailbox.id, "signup: display name write failed");
        }
    }

    let issued = match store
        .issue_token(
            tenant_id,
            Some(mailbox.id),
            SESSION_LABEL,
            Some(Duration::days(SESSION_TTL_DAYS)),
        )
        .await
    {
        Ok(issued) => issued,
        Err(err) => return unavailable(locale, err),
    };

    let object = format!("mailbox:{}", mailbox.id);
    let audit = audit_context(&meta, object.clone());
    if let Err(err) = store
        .record_auth_event(
            Some(tenant_id),
            &audit,
            "auth.signup",
            Some(object.as_str()),
            "ok",
            serde_json::json!({ "domain": domain, "source": "dev-signup" }),
        )
        .await
    {
        // Same rule as login: a session without its audit row must not exist.
        let _ = store.revoke_token(issued.record.id).await;
        return unavailable(locale, err);
    }

    let mut response = Redirect::to(&format!("/mail/{}", mailbox.id)).into_response();
    set_session_cookie(&mut response, &issued.secret);
    response
}

/// Lowercases and validates a local part. Returns `None` for anything the
/// schema or the address grammar would reject, so the caller renders one
/// sentence instead of a database error.
fn normalize_local_part(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    let len = value.chars().count();
    if !(MIN_LOCAL_PART..=MAX_LOCAL_PART).contains(&len) {
        return None;
    }
    let mut chars = value.chars();
    let first = chars.next()?;
    let last = value.chars().last()?;
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return None;
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return None;
    }
    Some(value)
}

/// A duplicate address arrives as a PostgreSQL unique violation. The mapping
/// lives here so the handler never has to string-match an error message.
fn is_conflict(err: &StoreError) -> bool {
    err.is_unique_violation()
}

#[cfg(test)]
mod tests {
    use super::{domain, normalize_local_part};

    #[test]
    fn local_parts_are_lowercased_and_validated() {
        assert_eq!(
            normalize_local_part("  Yassine ").as_deref(),
            Some("yassine")
        );
        assert_eq!(normalize_local_part("a.b-c_1").as_deref(), Some("a.b-c_1"));
        assert_eq!(normalize_local_part("yassine@evil.test"), None);
        assert_eq!(normalize_local_part(".hidden"), None);
        assert_eq!(normalize_local_part("trailing."), None);
        assert_eq!(normalize_local_part("a"), None);
        assert_eq!(normalize_local_part("with space"), None);
        assert_eq!(normalize_local_part(&"x".repeat(33)), None);
    }

    #[test]
    fn domain_defaults_when_unset_or_malformed() {
        // The test process owns its environment; unset is the common case.
        std::env::remove_var("TIRA_DEV_DOMAIN");
        assert_eq!(domain(), "tira.test");
    }
}
