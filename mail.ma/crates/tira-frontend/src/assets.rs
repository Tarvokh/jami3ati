//! Static assets, embedded at compile time.
//!
//! Embedding removes a whole class of deployment bugs (wrong working
//! directory, missing `static/` folder, path traversal) and makes the release
//! binary self-contained. The cost is a rebuild to change the CSS — acceptable
//! for a prototype with a single owner.

pub const APP_CSS: &str = include_str!("../assets/app.css");

/// HTMX 2.0.4, vendored from the official distribution on 2026-11-07.
/// SHA-256 `e209dda5c8235479f3166defc7750e1dbcd5a5c1808b7792fc2e6733768fb447`.
/// No CDN at runtime, ever.
pub const HTMX_JS: &str = include_str!("../assets/htmx.min.js");

/// Navigation-accessibility enhancer: a tiny self-hosted script that turns
/// HTMX's boosted swaps into screen-reader feedback (`aria-busy` plus a polite
/// live region). Progressive — the CSS indicator works without it, and with
/// JavaScript disabled there are no boosted swaps to enhance. No CDN, ever.
pub const NAV_A11Y_JS: &str = include_str!("../assets/nav-a11y.js");

/// One embedded webfont file.
///
/// The files are the Arabic and Latin subsets of IBM Plex Sans Arabic 4.0
/// (SIL Open Font License 1.1, `assets/fonts/OFL.txt`). They are served from
/// this process: no request ever leaves the deployment.
#[derive(Debug, Clone, Copy)]
pub struct FontAsset {
    pub file: &'static str,
    pub bytes: &'static [u8],
}

pub const FONT_ASSETS: &[FontAsset] = &[
    FontAsset {
        file: "plex-arabic-400-arabic.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-400-arabic.woff2"),
    },
    FontAsset {
        file: "plex-arabic-400-latin.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-400-latin.woff2"),
    },
    FontAsset {
        file: "plex-arabic-500-arabic.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-500-arabic.woff2"),
    },
    FontAsset {
        file: "plex-arabic-500-latin.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-500-latin.woff2"),
    },
    FontAsset {
        file: "plex-arabic-600-arabic.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-600-arabic.woff2"),
    },
    FontAsset {
        file: "plex-arabic-600-latin.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-600-latin.woff2"),
    },
    FontAsset {
        file: "plex-arabic-700-arabic.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-700-arabic.woff2"),
    },
    FontAsset {
        file: "plex-arabic-700-latin.woff2",
        bytes: include_bytes!("../assets/fonts/plex-arabic-700-latin.woff2"),
    },
];

/// Exact-name lookup. There is deliberately no path handling here: the route
/// takes a file name, and anything that is not in the embedded list is a 404.
pub fn font(file: &str) -> Option<&'static FontAsset> {
    FONT_ASSETS.iter().find(|asset| asset.file == file)
}

/// FNV-1a: stable across runs, cheap over tens of kilobytes, and used for
/// cache busting only — not as a security boundary.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Content-derived ETag. The stylesheet is served with `max-age=300`, so the
/// validator must change when the CSS changes; a crate-version-only ETag would
/// let a browser keep the previous design after an upgrade.
pub static CSS_ETAG: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| format!("\"app-css-{:016x}\"", fnv1a(APP_CSS.as_bytes())));

/// Fonts are immutable for a given file name, so their validator never needs
/// to change; it is still derived from the bytes to keep that promise honest.
pub fn font_etag(asset: &FontAsset) -> String {
    format!("\"font-{:016x}\"", fnv1a(asset.bytes))
}

pub const HTMX_ETAG: &str = "\"htmx-2.0.4\"";

/// Content-derived like the stylesheet: this script is ours and may evolve,
/// so its validator must change when its bytes change, not when HTMX does.
pub static NAV_A11Y_ETAG: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| format!("\"nav-a11y-{:016x}\"", fnv1a(NAV_A11Y_JS.as_bytes())));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_is_embedded_as_woff2() {
        assert_eq!(FONT_ASSETS.len(), 8);
        for asset in FONT_ASSETS {
            assert!(
                asset.bytes.starts_with(b"wOF2"),
                "{} is not a woff2 file",
                asset.file
            );
            assert!(font(asset.file).is_some());
        }
    }

    #[test]
    fn font_lookup_is_an_exact_name_not_a_path() {
        assert!(font("plex-arabic-400-arabic.woff2").is_some());
        assert!(font("../app.css").is_none());
        assert!(font("OFL.txt").is_none());
        assert!(font("").is_none());
    }

    #[test]
    fn etags_change_with_content() {
        let a = FontAsset {
            file: "a",
            bytes: b"one",
        };
        let b = FontAsset {
            file: "b",
            bytes: b"two",
        };
        assert_ne!(font_etag(&a), font_etag(&b));
        assert!(CSS_ETAG.starts_with("\"app-css-"));
    }

    #[test]
    fn nav_enhancer_is_embedded_self_hosted_and_wired_to_htmx() {
        assert!(!NAV_A11Y_JS.is_empty());
        // Self-hosted: it never references an external resource.
        assert!(!NAV_A11Y_JS.contains("https://"));
        assert!(!NAV_A11Y_JS.contains("http://"));
        // It drives the two shell elements and listens to HTMX's own events.
        assert!(NAV_A11Y_JS.contains("nav-status"));
        assert!(NAV_A11Y_JS.contains("aria-busy"));
        assert!(NAV_A11Y_JS.contains("htmx:beforeRequest"));
        assert!(NAV_A11Y_JS.contains("htmx:afterSettle"));
        assert!(NAV_A11Y_ETAG.starts_with("\"nav-a11y-"));
    }
}
