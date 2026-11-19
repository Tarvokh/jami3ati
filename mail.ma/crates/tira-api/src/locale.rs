//! Locale resolution and the language cookie.
//!
//! The order is explicit and testable: `?lang=` wins (the person just clicked a
//! switcher link), then the `tira_lang` cookie (their previous choice), then
//! `Accept-Language` (a browser default), then Arabic.
//!
//! A query choice is written back as a cookie so the next POST — which carries
//! no query string — still renders in the chosen language. The cookie is
//! `HttpOnly` and `SameSite=Lax`: nothing in JavaScript needs it, and a
//! top-level navigation from another site should keep it.

use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::Response;
use serde::Deserialize;
use tira_frontend::i18n::Locale;

/// Name of the language cookie.
pub const LANG_COOKIE: &str = "tira_lang";

/// One year, in seconds.
const LANG_TTL_SECONDS: i64 = 31_536_000;

/// `?lang=fr` on the login and signup routes.
#[derive(Debug, Deserialize)]
pub struct LangQuery {
    #[serde(default)]
    pub lang: Option<String>,
}

/// Resolves the locale for a request. Never fails: an unknown tag falls through
/// to the next source, and Arabic is the final default.
pub fn resolve(headers: &HeaderMap, query: Option<&str>) -> Locale {
    if let Some(locale) = query.and_then(Locale::from_code) {
        return locale;
    }
    if let Some(locale) = cookie_value(headers).and_then(|value| Locale::from_code(&value)) {
        return locale;
    }
    if let Some(value) = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
    {
        if let Some(locale) = Locale::from_accept_language(value) {
            return locale;
        }
    }
    Locale::Ar
}

/// Reads `tira_lang` from a `Cookie` header. The values are ASCII language tags,
/// so no percent-decoding is needed.
pub fn cookie_value(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == LANG_COOKIE).then(|| value.trim().to_string())
    })
}

/// The `Set-Cookie` value for a language choice.
pub fn cookie(locale: Locale, secure: bool) -> String {
    let mut value = format!(
        "{LANG_COOKIE}={}; Path=/; Max-Age={LANG_TTL_SECONDS}; SameSite=Lax; HttpOnly",
        locale.code()
    );
    if secure {
        value.push_str("; Secure");
    }
    value
}

/// Appends the language cookie to a response.
pub fn set_cookie(response: &mut Response, locale: Locale, secure: bool) {
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie(locale, secure)).expect("language cookie is ascii"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn query_wins_over_cookie_and_header() {
        let h = headers(&[
            ("cookie", "tira_lang=fr"),
            ("accept-language", "en-GB,en;q=0.9"),
        ]);
        assert_eq!(resolve(&h, Some("en")), Locale::En);
        assert_eq!(resolve(&h, Some("ar")), Locale::Ar);
        // Unknown query tag falls through to the cookie, not to the default.
        assert_eq!(resolve(&h, Some("de")), Locale::Fr);
    }

    #[test]
    fn cookie_wins_over_accept_language_and_defaults_to_arabic() {
        let h = headers(&[
            ("cookie", "a=1; tira_lang=en; b=2"),
            ("accept-language", "fr-FR,fr;q=0.9"),
        ]);
        assert_eq!(resolve(&h, None), Locale::En);
        assert_eq!(resolve(&HeaderMap::new(), None), Locale::Ar);
        assert_eq!(
            resolve(&headers(&[("accept-language", "fr-FR")]), None),
            Locale::Fr
        );
    }

    #[test]
    fn language_cookie_is_scoped_and_locked_down() {
        let secure = cookie(Locale::Fr, true);
        assert!(secure.starts_with("tira_lang=fr;"));
        assert!(secure.contains("Path=/"));
        assert!(secure.contains("SameSite=Lax"));
        assert!(secure.contains("HttpOnly"));
        assert!(secure.contains("; Secure"));
        assert!(secure.contains("Max-Age=31536000"));
        assert!(!secure.contains("Domain="));

        let plain = cookie(Locale::En, false);
        assert!(plain.contains("tira_lang=en"));
        assert!(!plain.contains("Secure"));
    }
}
