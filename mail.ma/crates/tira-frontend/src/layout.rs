//! Document shell.
//!
//! One `<head>` for the whole product: the document language and direction come
//! from the resolved [`Locale`], the referrer policy is strict, and the
//! stylesheet, the embedded webfont and two self-hosted scripts are the only
//! resources. No inline style or script, so the Content-Security-Policy served
//! by `tira-api` stays at `default-src 'self'` with no `'unsafe-inline'`. Both
//! scripts are progressive: with JavaScript disabled, links and forms are
//! ordinary full navigations and the CSS entrance still plays.

use maud::{html, Markup, DOCTYPE};

use crate::i18n::Locale;
use crate::PRODUCT;

/// HTMX injects a `<style>` block for its indicator classes by default. Under
/// a strict `style-src` that is a blocked resource, so it is turned off here
/// and the indicator rules live in `app.css` instead.
const HTMX_CONFIG: &str = r#"{"includeIndicatorStyles":false,"historyCacheSize":0}"#;

/// The mailbox shell in the requested locale.
pub fn document(locale: Locale, title: &str, body: Markup) -> Markup {
    document_with(locale, title, body, false)
}

/// The auth screens load one extra progressive enhancer (password reveal,
/// confirm-mismatch hint, Caps Lock warning) and carry the chosen language.
/// The mailbox shell does not load it: an extra request that can never do
/// anything is still a cost.
pub fn auth_document(locale: Locale, title: &str, body: Markup) -> Markup {
    document_with(locale, title, body, true)
}

fn document_with(locale: Locale, title: &str, body: Markup, auth: bool) -> Markup {
    // The skip link and the loading pill are the only chrome that lives in
    // `<body>` before the page content, so they need the same locale as the
    // page. The two tables carry the same two sentences; pick the matching one.
    let (skip_to_content, loading) = if auth {
        (locale.auth().skip_to_content, locale.auth().loading)
    } else {
        (locale.shell().skip_to_content, locale.shell().loading)
    };

    html! {
        (DOCTYPE)
        html lang=(locale.code()) dir=(locale.dir()) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="color-scheme" content="light dark";
                meta name="theme-color" content="#F7F8FA" media="(prefers-color-scheme: light)";
                meta name="theme-color" content="#0B1220" media="(prefers-color-scheme: dark)";
                meta name="referrer" content="no-referrer";
                meta name="htmx-config" content=(HTMX_CONFIG);
                meta name="description" content=(locale.meta_description());
                title { (title) " — " (PRODUCT) }
                // The regular Arabic + Latin faces are needed for first paint.
                // `crossorigin` is required for font preloads even same-origin.
                link rel="preload" href="/assets/fonts/plex-arabic-400-arabic.woff2"
                    as="font" type="font/woff2" crossorigin;
                link rel="preload" href="/assets/fonts/plex-arabic-400-latin.woff2"
                    as="font" type="font/woff2" crossorigin;
                link rel="stylesheet" href="/assets/app.css";
                script src="/assets/htmx.min.js" defer {}
                script src="/assets/nav-a11y.js" defer {}
                @if auth {
                    script src="/assets/auth.js" defer {}
                }
            }
            body hx-boost="true" hx-indicator="#nav-indicator" {
                a.skip-link href="#main" { (skip_to_content) }
                // Visual navigation feedback. aria-hidden because the sr-only
                // status region below is the screen-reader channel; HTMX marks
                // this pill .htmx-request for the duration of a boosted swap.
                div id="nav-indicator" class="htmx-indicator nav-indicator" aria-hidden="true" {
                    span.spinner {}
                    span { (loading) }
                }
                // Persistent polite live region: present in the accessibility
                // tree at rest (sr-only, not display:none) so nav-a11y.js can
                // announce the loading state and the arrived page title.
                p id="nav-status" class="sr-only" role="status" aria-live="polite" {}
                (body)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_is_rtl_and_self_hosted() {
        let html = document(Locale::Ar, "الوارد", html! { p { "مرحبا" } }).into_string();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("dir=\"rtl\""));
        assert!(html.contains("lang=\"ar\""));
        assert!(html.contains("href=\"/assets/app.css\""));
        assert!(html.contains("src=\"/assets/htmx.min.js\""));
        assert!(html.contains("name=\"referrer\" content=\"no-referrer\""));
        assert!(html.contains("name=\"color-scheme\" content=\"light dark\""));
        // Fonts are local, preloaded, and typed.
        assert!(html.contains(
            "href=\"/assets/fonts/plex-arabic-400-arabic.woff2\" as=\"font\" type=\"font/woff2\""
        ));
        assert!(html.contains("href=\"/assets/fonts/plex-arabic-400-latin.woff2\""));
        assert!(html.contains("href=\"#main\""));
        // Navigation feedback: a visual indicator plus a parallel live region,
        // and the self-hosted enhancer that drives the screen-reader channel.
        assert!(html.contains("hx-indicator=\"#nav-indicator\""));
        assert!(html.contains("id=\"nav-indicator\""));
        assert!(html.contains("class=\"spinner\""));
        assert!(html.contains("aria-hidden=\"true\""));
        assert!(html.contains("id=\"nav-status\""));
        assert!(html.contains("role=\"status\""));
        assert!(html.contains("aria-live=\"polite\""));
        assert!(html.contains("src=\"/assets/nav-a11y.js\""));
        // No CDN, no inline script, no external font.
        assert!(!html.contains("https://"));
        assert!(!html.contains("<style"));
    }

    #[test]
    fn title_is_escaped() {
        let html = document(Locale::Ar, "<script>", html! {}).into_string();
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn auth_pages_load_the_enhancer_and_other_pages_do_not() {
        let auth = auth_document(Locale::Ar, "الدخول", html! {}).into_string();
        assert!(auth.contains("src=\"/assets/auth.js\""));
        // Still self-hosted and still no inline script.
        assert!(!auth.contains("https://"));
        assert!(!auth.contains("<style"));

        let shell = document(Locale::Ar, "الوارد", html! {}).into_string();
        assert!(!shell.contains("auth.js"));
    }

    #[test]
    fn document_carries_the_locale_direction_and_chrome() {
        let fr = document(Locale::Fr, "Boîte de réception", html! {}).into_string();
        assert!(fr.contains("lang=\"fr\""));
        assert!(fr.contains("dir=\"ltr\""));
        assert!(fr.contains("Aller au contenu"));
        assert!(fr.contains("Chargement…"));
        assert!(fr.contains("interface de lecture sécurisée"));

        let en = auth_document(Locale::En, "Sign in", html! {}).into_string();
        assert!(en.contains("lang=\"en\""));
        assert!(en.contains("Skip to content"));
        assert!(en.contains("a secure reading interface"));

        let ar = document(Locale::Ar, "الوارد", html! {}).into_string();
        assert!(ar.contains("lang=\"ar\""));
        assert!(ar.contains("dir=\"rtl\""));
        assert!(ar.contains("انتقل إلى المحتوى"));
    }
}
