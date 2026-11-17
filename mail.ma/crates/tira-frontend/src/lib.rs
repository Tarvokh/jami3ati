//! Tira Mail server-rendered UI.
//!
//! Rendering is Rust: [Maud] builds HTML in compile-checked macros, so a typo
//! in an element name or a missing closing tag is a `cargo check` failure, not
//! a runtime surprise. The browser receives plain HTML plus one vendored
//! script (HTMX) that upgrades navigation; everything still works with
//! JavaScript disabled.
//!
//! Two rules hold across this crate:
//!
//! 1. **No fake data.** Every value passed to a page comes from the store,
//!    the request, or the clock. There is no sample mailbox, no placeholder
//!    count, no dead link.
//! 2. **Nothing pretends to work.** An implemented feature is a real, working
//!    control (composing a message links to the live compose route). A feature
//!    that is not implemented is never a button that silently does nothing: it
//!    is a disabled control or an honest labelled entry that states why
//!    (`title` / reason text).
//!
//! [Maud]: https://docs.rs/maud

pub mod assets;
pub mod components;
pub mod format;
pub mod icons;
pub mod layout;
pub mod pages;

pub use components::{
    CheckView, InboxModel, MailboxModel, MessageAuth, MessageModel, ProfileModel, ReadingMessage,
    ReadingModel, SentNotice, SessionModel,
};
pub use maud::Markup;

pub const PRODUCT: &str = "Tira Mail";
pub const ORG: &str = "TIRA LABS";

/// Renders the static page shells with empty data so the live-evidence
/// endpoint can scan them for external asset references. This is not a route:
/// nothing here is ever served to a user.
pub fn asset_scan_target() -> String {
    let mailbox = MailboxModel {
        id: String::new(),
        address: String::new(),
        display_name: None,
        total: 0,
        used_bytes: 0,
        quota_bytes: 1,
    };

    let mut out = pages::login(None).into_string();
    out.push_str(
        &pages::inbox(&InboxModel {
            mailbox,
            messages: Vec::new(),
            server_time: String::new(),
        })
        .into_string(),
    );
    out
}
