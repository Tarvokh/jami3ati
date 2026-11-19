//! Page-level composition.
//!
//! `inbox`, `inbox_with_reader` and `message_print` live in
//! [`crate::components`] because they share the sidebar and row components;
//! this module owns the screens that do not use the mailbox shell (login,
//! signup, service errors).

use maud::{html, Markup};

use crate::i18n::{AuthText, Locale};
use crate::icons::Icon;
use crate::layout::{auth_document, document};

pub use crate::components::{
    compose, inbox, inbox_with_notice, inbox_with_reader, message_print, profile, CheckView,
    MessageAuth, SentNotice,
};
pub use crate::components::{
    InboxModel, MailboxModel, MessageModel, ProfileModel, ReadingModel, SessionModel,
};

/// The product lockup: one definition, used by every authentication screen.
fn brand() -> Markup {
    html! {
        span.brand {
            span.brand__mark { (maud::PreEscaped(Icon::Mail.svg())) }
            span.brand__text {
                span.brand__name { "Tira Mail" }
                span.brand__org { "TIRA LABS" }
            }
        }
    }
}

/// Language switcher. Plain links: the server remembers the choice in a
/// cookie, so this works with JavaScript disabled and needs no redirect
/// script. The active language is text, not a dead link to the current page.
fn language_switch(locale: Locale, path: &str) -> Markup {
    html! {
        nav.lang aria-label=(locale.auth().language) {
            @for option in Locale::ALL {
                @if option == locale {
                    span.lang__item.lang__item--active aria-current="true" lang=(option.code()) {
                        (option.label())
                    }
                } @else {
                    a.lang__item href=(format!("{path}?lang={}", option.code()))
                        hreflang=(option.code()) lang=(option.code()) {
                        (option.label())
                    }
                }
            }
        }
    }
}

/// The shared frame: lockup and language switcher, one card, one quiet footer.
///
/// Nothing here names the database, the hash function, or the mail engine.
/// Those are implementation details; a person signing in needs an address
/// field, a password field, and a way out.
fn auth_shell(
    locale: Locale,
    path: &str,
    title: &str,
    card: Markup,
    footer_note: Option<&'static str>,
) -> Markup {
    let text = locale.auth();
    let body = html! {
        main.auth id="main" {
            div.auth__shell {
                div.auth__bar {
                    a.auth__brand-link href="/" aria-label="Tira Mail" { (brand()) }
                    (language_switch(locale, path))
                }
                section.auth__card { (card) }
                footer.auth__foot {
                    span { (text.copyright) }
                    @if let Some(note) = footer_note {
                        span.auth__foot-note { (note) }
                    }
                }
            }
        }
    };

    auth_document(locale, title, body)
}

fn caps_warning(text: &AuthText) -> Markup {
    html! {
        div.caps-warn data-caps="1" aria-live="polite" {
            span.i { (maud::PreEscaped(Icon::Info.svg())) }
            span { (text.caps_lock) }
        }
    }
}

fn login_card(locale: Locale, error: Option<&str>, with_signup: bool) -> Markup {
    let text = locale.auth();
    html! {
        header.auth__head {
            h1 { (text.heading_login) }
            p.auth__lead { (text.lead_login) }
        }

        @if let Some(message) = error {
            div.auth__error role="alert" {
                span.i { (maud::PreEscaped(Icon::Info.svg())) }
                span { (message) }
            }
        }

        form.auth__form method="post" action="/login" hx-boost="false" {
            input type="hidden" name="mode" value="password";

            div.field {
                label for="email" { (text.email) }
                input id="email" name="email" type="email"
                    required autofocus autocomplete="username"
                    inputmode="email" spellcheck="false"
                    placeholder="name@example.com" dir="ltr";
            }

            div.field {
                label for="password" { (text.password) }
                div.field__control {
                    input id="password" name="password" type="password"
                        data-reveal="1"
                        data-reveal-show=(text.reveal_show)
                        data-reveal-hide=(text.reveal_hide)
                        required autocomplete="current-password";
                }
                (caps_warning(text))
            }

            button class="btn btn--primary btn--block" type="submit" {
                (text.sign_in)
            }
        }

        details.auth__help {
            summary { (text.recovery_summary) }
            p { (text.recovery_body) }
            form method="post" action="/login" hx-boost="false" {
                input type="hidden" name="mode" value="token";
                div.field {
                    label for="token" { (text.token_label) }
                    div.field__control {
                        input id="token" name="token" type="password"
                            data-reveal="1"
                            data-reveal-show=(text.reveal_show)
                            data-reveal-hide=(text.reveal_hide)
                            autocomplete="off" spellcheck="false" dir="ltr";
                    }
                    span.field__hint { (text.token_hint) }
                }
                button class="btn btn--ghost btn--block" type="submit" {
                    (text.token_submit)
                }
            }
        }

        @if with_signup {
            p.auth__switch {
                span { (text.no_mailbox) }
                a href="/signup" { (text.create_mailbox_link) }
            }
        }
    }
}

/// Login with the local password, with an access token kept as the
/// bootstrap/recovery door.
///
/// The password is a **Tira** credential, not the mail engine's password. The
/// page says "your password" and nothing more: naming the storage engine in a
/// sign-in form is an implementation leak, not honesty.
pub fn login(locale: Locale, error: Option<&str>) -> Markup {
    login_page(locale, error, false)
}

/// Login with a visible door to local signup. Used only when the process runs
/// with `TIRA_DEV_MODE=1`; a normal deployment never renders a link to a route
/// that is not mounted, because a dead control is a lie.
pub fn login_with_signup(locale: Locale, error: Option<&str>) -> Markup {
    login_page(locale, error, true)
}

fn login_page(locale: Locale, error: Option<&str>, with_signup: bool) -> Markup {
    let text = locale.auth();
    let note = if with_signup {
        Some(text.demo_note)
    } else {
        None
    };
    auth_shell(
        locale,
        "/login",
        text.title_login,
        login_card(locale, error, with_signup),
        note,
    )
}

/// Values a person typed on the signup form, echoed back after a validation
/// error so nothing must be retyped. It never carries a password.
#[derive(Debug, Clone, Default)]
pub struct SignupValues {
    pub local: String,
    pub display_name: String,
}

fn signup_card(locale: Locale, domain: &str, values: &SignupValues, error: Option<&str>) -> Markup {
    let text = locale.auth();
    html! {
        header.auth__head {
            h1 { (text.heading_signup) }
            p.auth__lead { (text.lead_signup) }
        }

        @if let Some(message) = error {
            div.auth__error role="alert" {
                span.i { (maud::PreEscaped(Icon::Info.svg())) }
                span { (message) }
            }
        }

        form.auth__form method="post" action="/signup" hx-boost="false" {
            div.field {
                label for="local" { (text.local_part) }
                div.field__group {
                    input id="local" name="local" type="text"
                        required autofocus autocomplete="username"
                        spellcheck="false" autocapitalize="none"
                        minlength="2" maxlength="32"
                        pattern="[a-z0-9][a-z0-9._-]{1,31}"
                        value=(values.local);
                    span.field__suffix { "@" (domain) }
                }
                span.field__hint { (text.local_hint) }
            }

            div.field {
                div.field__label-row {
                    label for="display_name" { (text.display_name) }
                    span.field__aside { (text.optional) }
                }
                input id="display_name" name="display_name" type="text"
                    autocomplete="name" maxlength="80"
                    value=(values.display_name);
            }

            div.field {
                div.field__label-row {
                    label for="password" { (text.password) }
                    span.field__aside { (text.password_min) }
                }
                div.field__control {
                    input id="password" name="password" type="password"
                        data-reveal="1" data-pw="1"
                        data-reveal-show=(text.reveal_show)
                        data-reveal-hide=(text.reveal_hide)
                        required autocomplete="new-password"
                        minlength="12" maxlength="128";
                }
                span.field__hint { (text.password_hint) }
                (caps_warning(text))
            }

            div.field {
                label for="confirm" { (text.confirm) }
                div.field__control {
                    input id="confirm" name="confirm" type="password"
                        data-reveal="1" data-pw-confirm="1"
                        data-reveal-show=(text.reveal_show)
                        data-reveal-hide=(text.reveal_hide)
                        required autocomplete="new-password"
                        minlength="12" maxlength="128";
                }
                span.field__error data-pw-mismatch="1" { (text.mismatch) }
            }

            button class="btn btn--primary btn--block" type="submit" {
                (text.create_mailbox)
            }
        }

        p.auth__switch {
            span { (text.have_mailbox) }
            a href="/login" { (text.sign_in_link) }
        }
    }
}

/// Local-development signup. The page promises only what the form does: pick
/// an address, pick a password, start reading. It is mounted in dev mode only.
pub fn signup(locale: Locale, domain: &str, values: &SignupValues, error: Option<&str>) -> Markup {
    let text = locale.auth();
    auth_shell(
        locale,
        "/signup",
        text.title_signup,
        signup_card(locale, domain, values, error),
        Some(text.demo_note),
    )
}

/// Honest terminal state for an infrastructure failure: the process is alive
/// but cannot verify or read anything.
///
/// The technical detail is logged by the caller, never rendered: a database
/// driver's sentence is not something a person can act on, and printing it on
/// a page teaches nobody anything.
pub fn service_unavailable(locale: Locale, _detail: &str) -> Markup {
    let text = locale.shell();
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                section.auth__pitch {
                    h1.auth__title { (text.unavailable_heading) }
                    p.auth__lead {
                        (text.unavailable_body)
                    }
                }
                section.auth__card {
                    div.notice.notice--info role="note" {
                        span.i { (maud::PreEscaped(Icon::Info.svg())) }
                        span { (text.unavailable_help) }
                    }
                    a class="btn btn--primary btn--block" href="/login" { (text.retry) }
                }
            }
        }
    };

    document(locale, text.title_unavailable, body)
}

/// CSRF boundary failure: the browser sent a state-changing request from an
/// origin the server cannot trust (cross-site, or an opaque `Origin: null`).
/// Deliberately says nothing about accounts or data — only that the request's
/// origin was rejected and no action ran.
pub fn cross_origin_rejected(locale: Locale) -> Markup {
    let text = locale.shell();
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                section.auth__pitch {
                    span.auth__eyebrow { (text.csrf_eyebrow) }
                    h1.auth__title { (text.csrf_title) }
                    p.auth__lead {
                        (text.csrf_body)
                    }
                }
                section.auth__card {
                    div.notice role="alert" {
                        span.i { (maud::PreEscaped(Icon::Lock.svg())) }
                        span {
                            (text.csrf_notice)
                        }
                    }
                    a class="btn btn--ghost btn--block" href="/login" { (text.csrf_back) }
                }
            }
        }
    };

    document(locale, text.title_cross_origin, body)
}

/// Generic honest failure page. Used for malformed ids, missing mailboxes and
/// any state where saying "not found" is the correct, non-enumerating answer.
pub fn problem(locale: Locale, eyebrow: &str, title: &str, message: &str) -> Markup {
    let text = locale.shell();
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                section.auth__pitch {
                    span.auth__eyebrow { (eyebrow) }
                    h1.auth__title { (title) }
                    p.auth__lead { (message) }
                }
                section.auth__card {
                    div.notice role="alert" {
                        span.i { (maud::PreEscaped(Icon::Info.svg())) }
                        span {
                            (text.problem_notice)
                        }
                    }
                    a class="btn btn--ghost btn--block" href="/login" { (text.back_to_login) }
                }
            }
        }
    };

    document(locale, title, body)
}

/// A tenant-wide token is valid but cannot address one mailbox in a UI that
/// has no mailbox picker yet. The page says that instead of guessing a mailbox.
pub fn scope_notice(locale: Locale) -> Markup {
    let text = locale.shell();
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                section.auth__pitch {
                    span.auth__eyebrow { (text.scope_eyebrow) }
                    h1.auth__title { (text.scope_title) }
                    p.auth__lead {
                        (text.scope_body)
                    }
                }
                section.auth__card {
                    div.notice.notice--info role="note" {
                        span.i { (maud::PreEscaped(Icon::KeyRound.svg())) }
                        span {
                            (text.scope_notice)
                        }
                    }
                    form method="post" action="/logout" hx-boost="false" {
                        button class="btn btn--ghost btn--block" type="submit" { (text.logout) }
                    }
                }
            }
        }
    };

    document(locale, text.title_scope, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_offers_password_first_and_token_as_recovery() {
        let html = login(Locale::Ar, None).into_string();
        assert!(html.contains("تسجيل الدخول"));
        assert!(html.contains("name=\"email\""));
        assert!(html.contains("name=\"password\""));
        assert!(html.contains("autocomplete=\"current-password\""));
        assert!(html.contains("name=\"token\""));
        assert!(html.contains("mode\" value=\"token\""));
        assert!(html.contains("dir=\"rtl\""));
        assert!(html.contains("href=\"/login?lang=fr\""));
        assert!(html.contains("href=\"/login?lang=en\""));
        // No implementation vocabulary on a sign-in form.
        assert!(!html.contains("Stalwart"));
        assert!(!html.contains("PostgreSQL"));
        assert!(!html.contains("Argon2id"));
        assert!(!html.contains("RLS"));
        assert!(!html.contains("غير موصول بعد"));
        assert!(!html.contains("href=\"#\""));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn login_and_signup_render_in_french_and_english() {
        let fr = login(Locale::Fr, None).into_string();
        assert!(fr.contains("lang=\"fr\""));
        assert!(fr.contains("dir=\"ltr\""));
        assert!(fr.contains("Connexion"));
        assert!(fr.contains("Adresse e-mail"));
        assert!(fr.contains("Se connecter"));
        assert!(!fr.contains("تسجيل الدخول"));

        let en = signup(Locale::En, "tira.test", &SignupValues::default(), None).into_string();
        assert!(en.contains("lang=\"en\""));
        assert!(en.contains("dir=\"ltr\""));
        assert!(en.contains("Create a mailbox"));
        assert!(en.contains("Mailbox name"));
        assert!(en.contains("tira.test"));
        assert!(en.contains("data-pw-mismatch"));
        assert!(!en.contains("PostgreSQL"));
    }

    #[test]
    fn login_error_is_visible_and_escaped() {
        let html = login(Locale::Ar, Some("<bad token>")).into_string();
        assert!(html.contains("&lt;bad token&gt;"));
        assert!(html.contains("role=\"alert\""));
    }
}
