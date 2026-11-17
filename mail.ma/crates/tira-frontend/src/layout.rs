//! Document shell.
//!
//! One `<head>` for the whole product: RTL Arabic, strict referrer policy, the
//! self-hosted stylesheet, the embedded webfont, and two self-hosted scripts —
//! the vendored HTMX build plus a tiny navigation-accessibility enhancer. No
//! inline style or script, so the Content-Security-Policy served by `tira-api`
//! stays at `default-src 'self'` with no `'unsafe-inline'`. Both scripts are
//! progressive: with JavaScript disabled, links and forms are ordinary full
//! navigations and the CSS entrance still plays.

use maud::{html, Markup, DOCTYPE};

use crate::PRODUCT;

/// HTMX injects a `<style>` block for its indicator classes by default. Under
/// a strict `style-src` that is a blocked resource, so it is turned off here
/// and the indicator rules live in `app.css` instead.
const HTMX_CONFIG: &str = r#"{"includeIndicatorStyles":false,"historyCacheSize":0}"#;

pub fn document(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="ar" dir="rtl" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="color-scheme" content="light dark";
                meta name="theme-color" content="#F7F8FA" media="(prefers-color-scheme: light)";
                meta name="theme-color" content="#0B1220" media="(prefers-color-scheme: dark)";
                meta name="referrer" content="no-referrer";
                meta name="htmx-config" content=(HTMX_CONFIG);
                meta name="description" content="Tira Mail — واجهة قراءة البريد الآمنة لـ TIRA LABS";
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
            }
            body hx-boost="true" hx-indicator="#nav-indicator" {
                a.skip-link href="#main" { "تجاوز إلى المحتوى" }
                // Visual navigation feedback. aria-hidden because the sr-only
                // status region below is the screen-reader channel; HTMX marks
                // this pill .htmx-request for the duration of a boosted swap.
                div id="nav-indicator" class="htmx-indicator nav-indicator" aria-hidden="true" {
                    span.spinner {}
                    span { "جارٍ التحميل…" }
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
        let html = document("الوارد", html! { p { "مرحبا" } }).into_string();
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
        let html = document("<script>", html! {}).into_string();
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
