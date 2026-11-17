//! Self-hosted icon set.
//!
//! Every icon is a static [Lucide] (ISC) SVG inlined as a string, so the page
//! never calls a CDN and no icon ships a script or an external reference.
//!
//! Motion is deliberately **not** an icon concern. The earlier line-md family
//! animated with SVG SMIL (`<animate>`), which `prefers-reduced-motion` cannot
//! govern: a CSS reduced-motion block only neutralises CSS animations and
//! transitions, so SMIL kept moving for people who asked it to stop. That is a
//! hidden accessibility bug, so the SMIL family was removed outright. All
//! entrance, hover, and loading motion now lives in `assets/app.css`, where one
//! `prefers-reduced-motion` block governs every keyframe and transition.
//!
//! Icons are decorative: each carries `aria-hidden="true"`, and the accessible
//! name lives on the interactive element (`<button aria-label="…">`), never on
//! the SVG.
//!
//! [Lucide]: https://lucide.dev

/// Static icon. The returned markup is trusted and must not be escaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Inbox,
    Mail,
    Send,
    Archive,
    Trash,
    Settings,
    Search,
    Star,
    User,
    Shield,
    Bell,
    ChevronLeft,
    ChevronRight,
    Menu,
    LogOut,
    Clock,
    Refresh,
    KeyRound,
    Lock,
    Info,
    Download,
    ArrowRight,
    Reply,
    Forward,
    Paperclip,
    Database,
    Printer,
}

impl Icon {
    pub fn svg(self) -> &'static str {
        match self {
            Self::Inbox => LUCIDE_INBOX,
            Self::Mail => LUCIDE_MAIL,
            Self::Send => LUCIDE_SEND,
            Self::Archive => LUCIDE_ARCHIVE,
            Self::Trash => LUCIDE_TRASH,
            Self::Settings => LUCIDE_SETTINGS,
            Self::Search => LUCIDE_SEARCH,
            Self::Star => LUCIDE_STAR,
            Self::User => LUCIDE_USER,
            Self::Shield => LUCIDE_SHIELD,
            Self::Bell => LUCIDE_BELL,
            Self::ChevronLeft => LUCIDE_CHEVRON_LEFT,
            Self::ChevronRight => LUCIDE_CHEVRON_RIGHT,
            Self::Menu => LUCIDE_MENU,
            Self::LogOut => LUCIDE_LOG_OUT,
            Self::Clock => LUCIDE_CLOCK,
            Self::Refresh => LUCIDE_REFRESH,
            Self::KeyRound => LUCIDE_KEY_ROUND,
            Self::Lock => LUCIDE_LOCK,
            Self::Info => LUCIDE_INFO,
            Self::Download => LUCIDE_DOWNLOAD,
            Self::ArrowRight => LUCIDE_ARROW_RIGHT,
            Self::Reply => LUCIDE_REPLY,
            Self::Forward => LUCIDE_FORWARD,
            Self::Paperclip => LUCIDE_PAPERCLIP,
            Self::Database => LUCIDE_DATABASE,
            Self::Printer => LUCIDE_PRINTER,
        }
    }
}

/// Renders a decorative static icon. The caller supplies the accessible name
/// on the interactive element (a `<button aria-label="...">`), not here.
pub fn icon(icon: Icon) -> &'static str {
    icon.svg()
}

const LUCIDE_INBOX: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11"/></g></svg>"#;

const LUCIDE_MAIL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="m22 7l-8.991 5.727a2 2 0 0 1-2.009 0L2 7"/><rect width="20" height="16" x="2" y="4" rx="2"/></g></svg>"#;

const LUCIDE_SEND: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11zm7.318-19.539l-10.94 10.939"/></svg>"#;

const LUCIDE_ARCHIVE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><rect width="20" height="5" x="2" y="3" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8m-10 4h4"/></g></svg>"#;

const LUCIDE_TRASH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 11v6m4-6v6m5-11v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>"#;

const LUCIDE_SETTINGS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0a2.34 2.34 0 0 0 3.319 1.915a2.34 2.34 0 0 1 2.33 4.033a2.34 2.34 0 0 0 0 3.831a2.34 2.34 0 0 1-2.33 4.033a2.34 2.34 0 0 0-3.319 1.915a2.34 2.34 0 0 1-4.659 0a2.34 2.34 0 0 0-3.32-1.915a2.34 2.34 0 0 1-2.33-4.033a2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/><circle cx="12" cy="12" r="3"/></g></svg>"#;

const LUCIDE_SEARCH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="m21 21l-4.34-4.34"/><circle cx="11" cy="11" r="8"/></g></svg>"#;

const LUCIDE_STAR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.12 2.12 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.12 2.12 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.12 2.12 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.12 2.12 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.12 2.12 0 0 0 1.597-1.16z"/></svg>"#;

const LUCIDE_USER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><circle cx="12" cy="8" r="5"/><path d="M20 21a8 8 0 0 0-16 0"/></g></svg>"#;

const LUCIDE_SHIELD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12l2 2l4-4"/></g></svg>"#;

const LUCIDE_BELL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.268 21a2 2 0 0 0 3.464 0m-10.47-5.674A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/></svg>"#;

const LUCIDE_CHEVRON_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m15 18l-6-6l6-6"/></svg>"#;

const LUCIDE_CHEVRON_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m9 18l6-6l-6-6"/></svg>"#;

const LUCIDE_MENU: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16"/></svg>"#;

const LUCIDE_LOG_OUT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="m16 17 5-5-5-5"/><path d="M21 12H9"/></g></svg>"#;

const LUCIDE_CLOCK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></g></svg>"#;

const LUCIDE_REFRESH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></g></svg>"#;

const LUCIDE_KEY_ROUND: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/><circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/></g></svg>"#;

const LUCIDE_LOCK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></g></svg>"#;

const LUCIDE_INFO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/></g></svg>"#;

const LUCIDE_DOWNLOAD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/></g></svg>"#;

const LUCIDE_ARROW_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></g></svg>"#;

const LUCIDE_REPLY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="m9 17-5-5 5-5"/><path d="M20 18v-2a4 4 0 0 0-4-4H4"/></g></svg>"#;

const LUCIDE_FORWARD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="m15 17 5-5-5-5"/><path d="M4 18v-2a4 4 0 0 1 4-4h12"/></g></svg>"#;

const LUCIDE_PAPERCLIP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>"#;

const LUCIDE_DATABASE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5V19A9 3 0 0 0 21 19V5"/><path d="M3 12A9 3 0 0 0 21 12"/></g></svg>"#;

const LUCIDE_PRINTER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2"/><path d="M6 9V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v6"/><rect width="12" height="8" x="6" y="14" rx="1"/></g></svg>"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every icon the UI can render, in one list so the invariants below are
    /// checked against the whole set rather than a sample. The explicit length
    /// is a guard: adding a variant without listing it here fails to compile.
    const ALL: [Icon; 27] = [
        Icon::Inbox,
        Icon::Mail,
        Icon::Send,
        Icon::Archive,
        Icon::Trash,
        Icon::Settings,
        Icon::Search,
        Icon::Star,
        Icon::User,
        Icon::Shield,
        Icon::Bell,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::Menu,
        Icon::LogOut,
        Icon::Clock,
        Icon::Refresh,
        Icon::KeyRound,
        Icon::Lock,
        Icon::Info,
        Icon::Download,
        Icon::ArrowRight,
        Icon::Reply,
        Icon::Forward,
        Icon::Paperclip,
        Icon::Database,
        Icon::Printer,
    ];

    #[test]
    fn every_icon_is_a_self_contained_decorative_svg() {
        for icon in ALL {
            let svg = icon.svg();
            assert!(svg.starts_with("<svg "), "not an svg: {svg}");
            assert!(svg.ends_with("</svg>"), "unterminated svg: {svg}");
            assert!(svg.contains("viewBox=\"0 0 24 24\""), "no viewBox: {svg}");
            assert!(
                svg.contains("aria-hidden=\"true\""),
                "no aria-hidden: {svg}"
            );
            assert!(!svg.contains("http://www.w3.org/1999/xlink"));
        }
    }

    /// The reduced-motion guarantee: no icon may carry SMIL. A CSS
    /// `prefers-reduced-motion` block cannot stop `<animate>` or
    /// `<animateTransform>`, so any SMIL here would be ungovernable motion —
    /// the exact bug that removed the line-md family. This fails if it returns.
    #[test]
    fn no_icon_ships_ungovernable_smil_motion() {
        for icon in ALL {
            let svg = icon.svg();
            assert!(!svg.contains("<animate"), "SMIL animate in {svg}");
            assert!(
                !svg.contains("<animateTransform"),
                "SMIL transform in {svg}"
            );
            assert!(!svg.contains("<set"), "SMIL set in {svg}");
            assert!(!svg.contains("repeatCount"), "SMIL loop in {svg}");
            assert!(!svg.contains("<script"), "script in {svg}");
        }
    }
}
