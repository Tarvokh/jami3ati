//! Typed view models and the components that render them.
//!
//! A page is a pure function of its model and its [`Locale`]. The API layer
//! fills the model from the store and resolves the language; nothing here reads
//! a database, a clock, or an environment variable, which is what makes the
//! rendering testable without fixtures.
//!
//! Only interface chrome is translated. Message subjects, bodies, sender names
//! and addresses are the sender's words and are rendered exactly as received,
//! with `dir="auto"` so the browser picks a direction per string.

use maud::{html, Markup, PreEscaped};

use crate::format;
use crate::i18n::Locale;
use crate::icons::Icon;
use crate::layout::document;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Inbox,
    Message,
    Profile,
}

#[derive(Debug, Clone)]
pub struct MailboxModel {
    pub id: String,
    pub address: String,
    /// Optional label from the profile. Never an identity.
    pub display_name: Option<String>,
    pub total: i64,
    pub used_bytes: i64,
    pub quota_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct MessageModel {
    pub id: String,
    pub from_addr: String,
    pub subject: Option<String>,
    pub snippet: Option<String>,
    pub size_bytes: i64,
    pub received_label: String,
}

#[derive(Debug, Clone)]
pub struct InboxModel {
    pub mailbox: MailboxModel,
    pub messages: Vec<MessageModel>,
    pub server_time: String,
}

/// A one-shot confirmation of an SMTP handoff, issued by the send handler and
/// bound to one mailbox.
///
/// It states only what the server observed: the configured relay accepted the
/// message. It is **not** a delivery receipt, and it is not a Sent-folder entry
/// (the current schema has no Sent folder), so the UI must not imply either.
#[derive(Debug, Clone)]
pub struct SentNotice {
    pub to: String,
    pub subject: String,
    pub accepted_at: String,
    pub relay: String,
}

/// One mail-authentication verdict as rendered in the reader: the check id
/// (`spf`, `dkim.rsa`, `dmarc`, …), its honest status token, and the observed
/// detail. The frontend never invents a verdict: it renders what the API loaded
/// from the live evidence.
#[derive(Debug, Clone)]
pub struct CheckView {
    pub id: &'static str,
    pub status: &'static str,
    pub detail: String,
}

/// The sender domain's published DNS policy records (SPF/DKIM selectors/DMARC/
/// MX/MTA-STS/TLS-RPT), loaded by the API from the stored zone evidence. This
/// is **not** per-message authentication: nothing here proves that this message
/// carried a valid DKIM signature or passed SPF/DMARC at receipt. The human sees
/// only which records exist, and an unreadable domain renders the honest
/// unavailable state instead of a fake pass.
#[derive(Debug, Clone)]
pub struct MessageAuth {
    pub domain: String,
    pub checks: Vec<CheckView>,
}

#[derive(Debug, Clone)]
pub struct ReadingMessage {
    pub id: String,
    pub subject: Option<String>,
    pub from_addr: String,
    pub to_addr: String,
    pub message_id_hdr: Option<String>,
    pub size_bytes: i64,
    pub received_full: String,
    /// RFC 3339 timestamp for the `<time datetime>` attribute. `received_full`
    /// stays the human label; this is the machine-readable twin.
    pub received_iso: String,
    pub body: Option<String>,
    /// Sender-domain DNS policy records (SPF/DKIM/DMARC/MX), when the stored
    /// evidence for that domain was readable. This is not per-message
    /// authentication. `None` means "not available here", never "passed".
    pub authenticity: Option<MessageAuth>,
}

#[derive(Debug, Clone)]
pub struct ReadingModel {
    pub mailbox: MailboxModel,
    pub server_time: String,
    pub message: ReadingMessage,
}

/// One revocable credential scoped to the signed-in mailbox.
#[derive(Debug, Clone)]
pub struct SessionModel {
    pub id: String,
    pub label: String,
    pub created_label: String,
    pub last_used_label: String,
    pub expires_label: String,
    pub is_current: bool,
}

/// Everything the profile page shows. All values are real store facts or
/// request facts; there is no placeholder row.
#[derive(Debug, Clone)]
pub struct ProfileModel {
    pub mailbox: MailboxModel,
    pub display_name: Option<String>,
    pub address: String,
    pub created_label: String,
    pub message_count: i64,
    pub password_set: bool,
    /// `pending`, `active`, `failed`, or `disabled` — the mail-engine bridge
    /// state. The token is the stored fact; the label is [`Locale::engine_label`].
    pub engine_state: String,
    pub sessions: Vec<SessionModel>,
    pub current_token_id: String,
    pub status: Option<&'static str>,
    pub error: Option<&'static str>,
}

fn icon(i: Icon) -> Markup {
    html! { span.i { (PreEscaped(i.svg())) } }
}

/// Percent-encodes one query-string component for a compose deep link. The
/// reply recipient and subject are not secrets, but they still travel in the
/// URL, so they are encoded rather than interpolated raw.
fn encode_query_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let safe = byte.is_ascii_lowercase()
            || byte.is_ascii_uppercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'@');
        if safe {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

fn avatar(address: &str, large: bool) -> Markup {
    let class = if large {
        format!("avatar avatar--lg {}", format::avatar_class(address))
    } else {
        format!("avatar {}", format::avatar_class(address))
    };
    html! {
        span class=(class) aria-hidden="true" {
            (format::initial(format::local_part(address)))
        }
    }
}

fn brand(locale: Locale, mailbox: Option<&MailboxModel>) -> Markup {
    let title = locale.shell().inbox_title;
    match mailbox {
        Some(m) => html! {
            a.brand href=(format!("/mail/{}", m.id)) title=(title) {
                span.brand__mark { (PreEscaped(Icon::Mail.svg())) }
                span.brand__text {
                    span.brand__name { "Tira Mail" }
                    span.brand__org { "TIRA LABS" }
                }
            }
        },
        None => html! {
            div.brand {
                span.brand__mark { (PreEscaped(Icon::Mail.svg())) }
                span.brand__text {
                    span.brand__name { "Tira Mail" }
                    span.brand__org { "TIRA LABS" }
                }
            }
        },
    }
}

/// Three real links, each a full navigation. The relative `?lang=` keeps the
/// current path and needs no script; the handler turns the choice into a cookie
/// so the next form post stays in the same language.
fn lang_switcher(locale: Locale) -> Markup {
    let text = locale.shell();
    html! {
        nav.lang aria-label=(text.language) {
            @for candidate in Locale::ALL {
                a class=(if candidate == locale { "lang__item lang__item--active" } else { "lang__item" })
                    href=(format!("?lang={}", candidate.code()))
                    hreflang=(candidate.code())
                    lang=(candidate.code())
                    aria-current=[(candidate == locale).then_some("true")] {
                    (candidate.native_name())
                }
            }
        }
    }
}

fn topbar(locale: Locale, mailbox: Option<&MailboxModel>) -> Markup {
    let text = locale.shell();
    html! {
        header.topbar {
            (brand(locale, mailbox))

            div.topbar__actions {
                (lang_switcher(locale))
                @if let Some(m) = mailbox {
                    a class="chip chip--account" href="/profile"
                        title=(locale.profile_chip_title(&m.address)) {
                        (avatar(&m.address, false))
                        span dir="auto" { (m.display_name.as_deref().unwrap_or_else(|| format::local_part(&m.address))) }
                    }
                }
                @if mailbox.is_some() {
                    form method="post" action="/logout" hx-boost="false" {
                        button type="submit" class="icon-btn" aria-label=(text.logout)
                            title=(text.logout_title) {
                            (icon(Icon::LogOut))
                        }
                    }
                }
            }
        }
    }
}

fn sidebar(locale: Locale, mailbox: &MailboxModel, active: Section) -> Markup {
    let text = locale.shell();
    let inbox_active = matches!(active, Section::Inbox | Section::Message);
    let used_pct = format::pct(mailbox.used_bytes, mailbox.quota_bytes).round() as u32;

    html! {
        aside.sidebar aria-label=(text.nav_label) {
            a class="btn btn--block" href=(format!("/mail/{}/compose", mailbox.id))
                title=(text.compose_new_title) {
                (icon(Icon::Send))
                span { (text.compose_new) }
            }

            nav.folders aria-label=(text.folders_label) {
                a class=(if inbox_active { "folder folder--active" } else { "folder" })
                    href=(format!("/mail/{}", mailbox.id))
                    aria-current=[inbox_active.then_some("page")]
                    title=(text.inbox_title) {
                    (icon(Icon::Inbox))
                    span { (text.inbox) }
                    span.folder__count { (mailbox.total) }
                }
                a class=(if active == Section::Profile { "folder folder--active" } else { "folder" })
                    href="/profile"
                    aria-current=[(active == Section::Profile).then_some("page")]
                    title=(text.profile_title) {
                    (icon(Icon::User))
                    span { (text.profile) }
                }
            }

            // Everything the schema or the mail engine cannot support yet lives
            // in one collapsed, honest list instead of six dashed buttons
            // shouting "coming soon" across the screen.
            details.unavailable {
                summary {
                    span { (text.soon_heading) }
                    span.unavailable__chev aria-hidden="true" {}
                }
                ul {
                    li { span { (text.soon_search) } span { (text.soon_search_why) } }
                    li { span { (text.soon_starred) } span { (text.soon_starred_why) } }
                    li { span { (text.soon_sent) } span { (text.soon_sent_why) } }
                    li { span { (text.soon_archive) } span { (text.soon_archive_why) } }
                    li { span { (text.soon_trash) } span { (text.soon_trash_why) } }
                }
            }

            div.quota {
                div.quota__head {
                    span { (text.storage) }
                    strong {
                        (format::bytes(locale, mailbox.used_bytes))
                        " / "
                        (format::bytes(locale, mailbox.quota_bytes))
                    }
                }
                progress
                    value=(mailbox.used_bytes.max(0))
                    max=(mailbox.quota_bytes.max(1))
                    aria-label=(locale.storage_used(used_pct)) {}
            }

            div.sidebar__foot {
                div.row { (icon(Icon::Inbox)) span { (text.foot_no_sync) } }
                div.row { (icon(Icon::Shield)) span { (text.foot_session_revocable) } }
            }
        }
    }
}

fn app_shell(
    locale: Locale,
    title: &str,
    mailbox: &MailboxModel,
    active: Section,
    content: Markup,
) -> Markup {
    document(
        locale,
        title,
        html! {
            (topbar(locale, Some(mailbox)))
            div.workspace {
                (sidebar(locale, mailbox, active))
                main.content id="main" { (content) }
            }
        },
    )
}

/// The two-pane shell: the inbox list and the reading pane share one page.
///
/// `view` is `inbox` or `message` and is the only thing the narrow layout needs
/// to know — below the split breakpoint exactly one pane is shown, so a phone
/// keeps the single-pane behaviour it had before. No JavaScript participates:
/// both panes are server-rendered, and `data-view` is a plain attribute.
fn shell_split(
    locale: Locale,
    title: &str,
    mailbox: &MailboxModel,
    active: Section,
    view: &str,
    list: Markup,
    reader: Markup,
) -> Markup {
    document(
        locale,
        title,
        html! {
            (topbar(locale, Some(mailbox)))
            div class="workspace workspace--split" data-view=(view) {
                (sidebar(locale, mailbox, active))
                main.panes id="main" {
                    div.pane.pane--list { (list) }
                    section.pane.pane--reader aria-label=(locale.shell().reader_label) { (reader) }
                }
            }
        },
    )
}

/// The reply/compose screen. This is the target of the `POST /mail/{id}/send`
/// form: the human owns the words, the server owns the envelope (sender = the
/// authenticated mailbox, subject defaults to `Re:`).
///
/// `error` carries an already-localized server refusal (validation, throttle,
/// upstream). The draft is preserved so a human does not retype a long body
/// after a failed attempt.
pub fn compose(
    locale: Locale,
    mailbox: &MailboxModel,
    to: &str,
    subject: &str,
    body: &str,
    error: Option<&str>,
) -> Markup {
    let text = locale.shell();
    let action = format!("/mail/{}/send", mailbox.id);
    let cancel = format!("/mail/{}", mailbox.id);

    let content = html! {
        div.section-head {
            h1 { (text.compose_heading) }
            span.section-head__meta {
                (text.sender_label)
                span.mono dir="ltr" { (mailbox.address) }
            }
        }

        div.panel {
            @if let Some(error) = error {
                div.compose__error role="alert" {
                    span.i { (maud::PreEscaped(Icon::Info.svg())) }
                    span { (error) }
                }
            }
            form method="post" action=(action) hx-boost="false" {
                div.field {
                    label for="compose-to" { (text.to) }
                    input id="compose-to" name="to" type="email" required
                        value=(to) autocomplete="email" spellcheck="false" dir="ltr";
                }
                div.field {
                    label for="compose-subject" { (text.subject) }
                    // A subject is content, not an address: `dir="auto"` lets an
                    // Arabic subject read right-to-left and a Latin one left-to-
                    // right, without the page deciding for the sender.
                    input id="compose-subject" name="subject" value=(subject) dir="auto";
                }
                div.field {
                    label for="compose-body" { (text.body) }
                    // `textarea` is not a void element: terminating it with `;`
                    // emits an unclosed start tag, and every following node —
                    // including the submit button — is swallowed into the
                    // field's text. The draft goes *inside* the element.
                    textarea id="compose-body" name="body" rows="10" required
                        spellcheck="false" dir="auto" { (body) }
                }
                div.form-actions {
                    button class="btn btn--primary" type="submit" { (text.send) }
                    a class="btn btn--ghost" href=(cancel) { (text.cancel) }
                }
            }
        }

        p.muted {
            (text.compose_note)
        }
    };

    app_shell(
        locale,
        text.title_compose,
        mailbox,
        Section::Message,
        content,
    )
}

/// The list half of the workspace. `selected` marks the row whose message is
/// open in the reader pane, so the current row is announced with
/// `aria-current="page"` instead of looking exactly like its neighbours.
/// `sent` is the one-shot SMTP-handoff confirmation, when the URL carried one.
fn inbox_list(
    locale: Locale,
    model: &InboxModel,
    selected: Option<&str>,
    sent: Option<&SentNotice>,
) -> Markup {
    let text = locale.shell();
    let reload = format!("/mail/{}", model.mailbox.id);

    html! {
        div.section-head {
            h1 { (text.inbox) }
            span.section-head__meta {
                (locale.messages_count(model.mailbox.total))
                " · "
                (text.last_updated)
                " "
                span.mono { (model.server_time) }
            }
            div.section-head__actions {
                a class="btn btn--ghost" href=(reload)
                    title=(text.reload_title) {
                    (icon(Icon::Refresh))
                    (text.refresh)
                }
            }
        }

        @if let Some(notice) = sent {
            div.sent-notice role="status" {
                div.sent-notice__head {
                    (icon(Icon::Send))
                    strong { (text.sent_heading) }
                }
                dl.sent-notice__facts {
                    div { dt { (text.to) } dd dir="ltr" { (notice.to) } }
                    div { dt { (text.subject) } dd dir="auto" { (notice.subject) } }
                    div { dt { (text.sent_at) } dd { (notice.accepted_at) } }
                }
                p.sent-notice__note {
                    (text.sent_note)
                }
            }
        }

        @if model.messages.is_empty() {
            div.panel {
                div.empty {
                    (icon(Icon::Inbox))
                    h2 { (text.empty_heading) }
                    p {
                        (text.empty_body)
                    }
                    a class="btn btn--primary empty__action"
                        href=(format!("/mail/{}/compose?to={}", model.mailbox.id, model.mailbox.address)) {
                        (text.send_to_self)
                    }
                }
            }
        } @else {
            div.panel {
                div.list {
                    @for message in &model.messages {
                        (message_row(
                            locale,
                            &model.mailbox.id,
                            message,
                            selected == Some(message.id.as_str()),
                        ))
                    }
                }
            }
        }

        p.list-note {
            (icon(Icon::Database))
            span { (text.no_fixtures) }
        }
    }
}

/// The empty half of the reader pane on the inbox screen. It is a layout
/// affordance, not data: it names the gesture the list already supports.
fn reader_placeholder(locale: Locale) -> Markup {
    let text = locale.shell();
    html! {
        div.reader-empty {
            (icon(Icon::Inbox))
            h2 { (text.choose_message) }
            p { (text.choose_message_body) }
        }
    }
}

pub fn inbox(locale: Locale, model: &InboxModel) -> Markup {
    inbox_with_notice(locale, model, None)
}

/// The inbox with a one-shot SMTP-handoff confirmation above the list.
///
/// The notice is issued by the send handler and looked up by an unguessable id
/// bound to the mailbox. It is rendered from server facts only; the template
/// never says the message reached the recipient, and it says explicitly that no
/// Sent folder exists yet.
pub fn inbox_with_notice(locale: Locale, model: &InboxModel, sent: Option<&SentNotice>) -> Markup {
    shell_split(
        locale,
        locale.shell().title_inbox,
        &model.mailbox,
        Section::Inbox,
        "inbox",
        inbox_list(locale, model, None, sent),
        reader_placeholder(locale),
    )
}

/// The inbox with one message open beside it. The list is the same list the
/// inbox route renders, with the open row marked as current; the reader is the
/// same reading view the message route has always rendered.
pub fn inbox_with_reader(locale: Locale, inbox: &InboxModel, reading: &ReadingModel) -> Markup {
    let subject = reading
        .message
        .subject
        .as_deref()
        .unwrap_or(locale.shell().no_subject);
    shell_split(
        locale,
        subject,
        &inbox.mailbox,
        Section::Message,
        "message",
        inbox_list(locale, inbox, Some(reading.message.id.as_str()), None),
        reader(locale, reading),
    )
}

fn message_row(
    locale: Locale,
    mailbox_id: &str,
    message: &MessageModel,
    is_current: bool,
) -> Markup {
    let href = format!("/mail/{mailbox_id}/message/{}", message.id);
    let subject = message
        .subject
        .as_deref()
        .unwrap_or(locale.shell().no_subject);

    html! {
        a class=(if is_current { "row-link row-link--current" } else { "row-link" })
            href=(href)
            aria-current=[is_current.then_some("page")] {
            (avatar(&message.from_addr, false))
            div.row-main {
                div.row-top {
                    span.row-from dir="auto" { (format::local_part(&message.from_addr)) }
                    span.row-addr dir="ltr" { (message.from_addr) }
                }
                div.row-bottom {
                    span.row-subject dir="auto" { (subject) }
                    @if let Some(snippet) = &message.snippet {
                        span.row-snippet dir="auto" { (snippet) }
                    }
                }
            }
            div.row-side {
                span.row-time { (message.received_label) }
                span.row-size { (format::bytes(locale, message.size_bytes)) }
            }
        }
    }
}

fn reader(locale: Locale, model: &ReadingModel) -> Markup {
    let text = locale.shell();
    let back = format!("/mail/{}", model.mailbox.id);
    let raw = format!("/api/v1/messages/{}/raw", model.message.id);
    let print = format!(
        "/mail/{}/message/{}/print",
        model.mailbox.id, model.message.id
    );
    let subject = model.message.subject.as_deref().unwrap_or(text.no_subject);
    let sender = format::local_part(&model.message.from_addr);
    let domain = format::domain_part(&model.message.from_addr);
    let reply_subject = format!("Re: {}", model.message.subject.as_deref().unwrap_or(""));
    let reply_href = format!(
        "/mail/{}/compose?to={}&subject={}",
        model.mailbox.id,
        encode_query_component(&model.message.from_addr),
        encode_query_component(&reply_subject),
    );

    html! {
        div.reader {
            header.reader-bar aria-label=(text.message_tools) {
                a.back-link href=(back) title=(text.back_to_inbox_title) {
                    (icon(Icon::ArrowRight))
                    (text.back_to_inbox)
                }
                div.reader-bar__context {
                    span.reader-bar__label { (text.message_label) }
                    span.reader-bar__subject dir="auto" { (subject) }
                }
                div.reader-bar__actions {
                    a class="btn btn--ghost reader-bar__btn" href=(print)
                        aria-label=(text.print)
                        title=(text.print_title) {
                        (icon(Icon::Printer))
                        span.reader-bar__btn-text { (text.print) }
                    }
                    a class="btn btn--ghost reader-bar__btn" href=(raw) download="message.eml"
                        aria-label=(text.download_eml)
                        title=(text.download_eml_title) {
                        (icon(Icon::Download))
                        span.reader-bar__btn-text { (text.download_eml) }
                    }
                    a class="btn btn--ghost reader-bar__btn" href=(reply_href)
                        aria-label=(text.reply)
                        title=(text.reply_title) {
                        (icon(Icon::Reply))
                        span.reader-bar__btn-text { (text.reply) }
                    }
                }
            }

            div.reader-grid {
                article.panel.reader-main aria-labelledby="message-subject" {
                    h1.reader__subject id="message-subject" dir="auto" { (subject) }

                    p.reader__to {
                        span.reader__to-label { (text.to) }
                        span.reader__to-addr dir="ltr" { (model.message.to_addr) }
                    }

                    @if let Some(body) = &model.message.body {
                        div.reader__body dir="auto" { (body) }
                    } @else {
                        div.reader__body.muted {
                            (text.body_unavailable)
                        }
                    }

                    p.article__note role="note" {
                        (icon(Icon::Lock))
                        span {
                            (text.html_disabled)
                        }
                    }

                    details.unavailable {
                        summary {
                            span { (text.attachments_unavailable) }
                            span.unavailable__chev aria-hidden="true" {}
                        }
                        ul {
                            li { span { (text.attachments) } span { (text.attachments_why) } }
                        }
                    }
                }

                aside.reader-rail aria-label=(text.message_details) {
                    section.panel.rail-card {
                        header.rail-card__head {
                            (avatar(&model.message.from_addr, true))
                            div.rail-card__who {
                                span.rail-card__name dir="auto" { (sender) }
                                span.rail-card__addr dir="ltr" { (model.message.from_addr) }
                                @if !domain.is_empty() {
                                    span.rail-card__domain dir="ltr" { (domain) }
                                }
                            }
                        }

                        dl.rail-meta {
                            div {
                                dt { (text.received_at) }
                                dd {
                                    time datetime=(model.message.received_iso) {
                                        (model.message.received_full)
                                    }
                                }
                            }
                            div {
                                dt { (text.size) }
                                dd { (format::bytes(locale, model.message.size_bytes)) }
                            }
                            @if let Some(message_id) = &model.message.message_id_hdr {
                                div {
                                    dt { "Message-ID" }
                                    dd.mono dir="ltr" { (message_id) }
                                }
                            } @else {
                                div {
                                    dt { "Message-ID" }
                                    dd.muted { (text.no_message_id) }
                                }
                            }
                            div {
                                dt { (text.identifier) }
                                dd.mono dir="ltr" { (model.message.id) }
                            }
                        }

                    @if let Some(auth) = &model.message.authenticity {
                        section.panel.rail-card.reader-auth {
                            header.rail-card__head {
                                (icon(Icon::Shield))
                                div.rail-card__who {
                                    span.rail-card__name { (text.sender_domain_records) }
                                    span.rail-card__addr dir="ltr" { (auth.domain) }
                                }
                            }
                            p.reader-auth__scope {
                                (text.auth_scope)
                            }
                            ul.reader-auth__checks {
                                @for check in &auth.checks {
                                    li class=(format!("reader-auth__check reader-auth__check--{}", check.status)) {
                                        span.reader-auth__check-id { (check.id) }
                                        span.reader-auth__check-status { (locale.check_status(check.status)) }
                                        p.reader-auth__check-detail { (check.detail) }
                                    }
                                }
                            }
                        }
                    } @else {
                        section.panel.rail-card.reader-auth {
                            header.rail-card__head {
                                (icon(Icon::Shield))
                                div.rail-card__who {
                                    span.rail-card__name { (text.sender_domain_records) }
                                    span.rail-card__addr dir="ltr" { (domain) }
                                }
                            }
                            p.reader-auth__missing {
                                (text.auth_missing)
                            }
                        }
                    }

                        p.rail-note {
                            (icon(Icon::Database))
                            span {
                                (text.message_no_fixtures)
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A paper-first view of the same message. It is a real navigation target, not
/// a `window.print()` trick, so it works with JavaScript disabled and under the
/// strict CSP. The on-screen toolbar disappears in `@media print`.
pub fn message_print(locale: Locale, model: &ReadingModel) -> Markup {
    let text = locale.shell();
    let back = format!("/mail/{}/message/{}", model.mailbox.id, model.message.id);
    let subject = model.message.subject.as_deref().unwrap_or(text.no_subject);

    document(
        locale,
        subject,
        html! {
            div.print-page {
                div.print-bar {
                    a.back-link href=(back) title=(text.back_to_message_title) {
                        (icon(Icon::ArrowRight))
                        (text.back_to_message)
                    }
                    p.print-bar__hint {
                        (text.print_hint_before)
                        span.mono dir="ltr" { "Ctrl+P" }
                        (text.print_hint_or)
                        span.mono dir="ltr" { "⌘P" }
                        (text.print_hint_after)
                    }
                }

                article.print-sheet aria-labelledby="print-subject" {
                    header.print-sheet__head {
                        h1 id="print-subject" dir="auto" { (subject) }
                        dl.print-meta {
                            div {
                                dt { (text.from) }
                                dd dir="ltr" { (model.message.from_addr) }
                            }
                            div {
                                dt { (text.to) }
                                dd dir="ltr" { (model.message.to_addr) }
                            }
                            div {
                                dt { (text.date) }
                                dd {
                                    time datetime=(model.message.received_iso) {
                                        (model.message.received_full)
                                    }
                                }
                            }
                            div {
                                dt { (text.size) }
                                dd { (format::bytes(locale, model.message.size_bytes)) }
                            }
                            @if let Some(message_id) = &model.message.message_id_hdr {
                                div {
                                    dt { "Message-ID" }
                                    dd.mono dir="ltr" { (message_id) }
                                }
                            }
                        }
                    }

                    div.print-sheet__body dir="auto" {
                        @if let Some(body) = &model.message.body {
                            (body)
                        } @else {
                            p.muted {
                                (text.body_unavailable)
                            }
                        }
                    }

                    footer.print-sheet__foot {
                        span { "Tira Mail — TIRA LABS" }
                        span.mono dir="ltr" { (model.message.id) }
                    }
                }
            }
        },
    )
}

/// The profile screen. Everything here is either a store fact or a form whose
/// action exists. No control is decorative: if it cannot work, it is absent and
/// the page says why.
pub fn profile(locale: Locale, model: &ProfileModel) -> Markup {
    let text = locale.shell();
    let label = model
        .display_name
        .as_deref()
        .unwrap_or_else(|| format::local_part(&model.address));
    let password_heading = if model.password_set {
        text.change_password
    } else {
        text.set_password
    };
    let engine = locale.engine_label(&model.engine_state);
    let other_sessions = model
        .sessions
        .iter()
        .filter(|session| !session.is_current)
        .count();

    let content = html! {
        div.section-head {
            h1 { (text.profile) }
            span.section-head__meta { (text.account_security) }
        }

        @if let Some(text) = model.status {
            div.notice.notice--ok role="status" {
                span.i { (icon(Icon::Info)) }
                span { (text) }
            }
        }
        @if let Some(text) = model.error {
            div.notice.notice--error role="alert" {
                span.i { (icon(Icon::Info)) }
                span { (text) }
            }
        }

        div.profile-grid {
            div.profile-main {
                section.panel.profile-section aria-labelledby="identity-title" {
                    div.profile-card__head {
                        (avatar(&model.address, true))
                        div.profile-card__who {
                            h2 id="identity-title" dir="auto" { (label) }
                            p.mono dir="ltr" { (model.address) }
                        }
                    }

                    dl.profile-facts {
                        div {
                            dt { (text.email) }
                            dd.mono dir="ltr" { (model.address) }
                        }
                        div {
                            dt { (text.created) }
                            dd { (model.created_label) }
                        }
                        div {
                            dt { (text.messages) }
                            dd { (model.message_count) }
                        }
                        div {
                            dt { (text.storage) }
                            dd {
                                (format::bytes(locale, model.mailbox.used_bytes))
                                " / "
                                (format::bytes(locale, model.mailbox.quota_bytes))
                            }
                        }
                        div {
                            dt { (text.mail_app_access) }
                            dd { (engine) }
                        }
                        div {
                            dt { (text.tira_password) }
                            dd {
                                @if model.password_set {
                                    (text.password_set)
                                } @else {
                                    (text.password_not_set)
                                }
                            }
                        }
                    }
                }

                section.panel.profile-section aria-labelledby="name-title" {
                    div.profile-section__head {
                        h2 id="name-title" { (text.display_name) }
                        p {
                            (text.display_name_help)
                        }
                    }
                    form method="post" action="/profile/display-name" hx-boost="false" {
                        div.field {
                            label for="display_name" { (text.name) }
                            input id="display_name" name="display_name" type="text"
                                class="input--text"
                                value=(model.display_name.as_deref().unwrap_or(""))
                                maxlength="80" autocomplete="nickname"
                                placeholder=(text.name_placeholder);
                            span.field__hint {
                                (text.name_hint)
                            }
                        }
                        button class="btn btn--primary" type="submit" { (text.save_name) }
                    }
                }

                section.panel.profile-section aria-labelledby="password-title" {
                    div.profile-section__head {
                        h2 id="password-title" { (password_heading) }
                        p {
                            (text.password_help)
                        }
                    }

                    @if !model.password_set {
                        div.notice.notice--info role="note" {
                            span.i { (icon(Icon::Info)) }
                            span {
                                (text.no_password_notice)
                            }
                        }
                    }

                    form method="post" action="/profile/password" hx-boost="false" {
                        @if model.password_set {
                            div.field {
                                label for="current_password" { (text.current_password) }
                                input id="current_password" name="current_password"
                                    type="password" autocomplete="current-password"
                                    required;
                            }
                        }
                        div.field {
                            label for="new_password" { (text.new_password) }
                            input id="new_password" name="new_password"
                                type="password" autocomplete="new-password"
                                required minlength="12" maxlength="128";
                            span.field__hint {
                                (text.password_rules)
                            }
                        }
                        div.field {
                            label for="confirm_password" { (text.confirm_password) }
                            input id="confirm_password" name="confirm_password"
                                type="password" autocomplete="new-password"
                                required minlength="12" maxlength="128";
                        }
                        button class="btn btn--primary" type="submit" { (password_heading) }
                    }
                }
            }

            aside.profile-side {
                section.panel.profile-section aria-labelledby="sessions-title" {
                    div.profile-section__head {
                        h2 id="sessions-title" { (text.active_sessions) }
                        p {
                            (text.sessions_help)
                        }
                    }

                    @if model.sessions.is_empty() {
                        p.muted { (text.no_sessions) }
                    } @else {
                        ul.session-list {
                            @for session in &model.sessions {
                                li.session {
                                    div.session__main {
                                        div.session__head {
                                            span.session__label { (session.label) }
                                            @if session.is_current {
                                                span.badge { (text.this_session) }
                                            }
                                        }
                                        dl.session__meta {
                                            div {
                                                dt { (text.session_created) }
                                                dd { (session.created_label) }
                                            }
                                            div {
                                                dt { (text.last_used) }
                                                dd { (session.last_used_label) }
                                            }
                                            div {
                                                dt { (text.expires) }
                                                dd { (session.expires_label) }
                                            }
                                        }
                                    }
                                    @if !session.is_current {
                                        form method="post" action="/profile/sessions/revoke" hx-boost="false" {
                                            input type="hidden" name="session_id" value=(session.id);
                                            button class="btn btn--ghost btn--danger" type="submit" {
                                                (text.revoke)
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        @if other_sessions > 0 {
                            form method="post" action="/profile/sessions/revoke-others" hx-boost="false" {
                                button class="btn btn--ghost btn--block" type="submit" {
                                    (text.revoke_others)
                                }
                            }
                        }
                    }
                }

                section.panel.profile-section {
                    div.profile-section__head {
                        h2 { (text.where_password_works) }
                    }
                    p.muted {
                        (text.where_password_body)
                    }
                }
            }
        }

        p.list-note {
            (icon(Icon::Shield))
            span { (text.session_trust_note) }
        }
    };

    app_shell(
        locale,
        text.title_profile,
        &model.mailbox,
        Section::Profile,
        content,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mailbox() -> MailboxModel {
        MailboxModel {
            id: "33333333-3333-4333-8333-333333333333".to_string(),
            address: "yassine@tira.test".to_string(),
            display_name: None,
            total: 2,
            used_bytes: 119,
            quota_bytes: 5 * 1024 * 1024,
        }
    }

    fn row(id: &str, subject: &str) -> MessageModel {
        MessageModel {
            id: id.to_string(),
            from_addr: "aicha@uca.ma".to_string(),
            subject: Some(subject.to_string()),
            snippet: Some("Salam Yassine".to_string()),
            size_bytes: 119,
            received_label: "10:42".to_string(),
        }
    }

    #[test]
    fn inbox_renders_real_rows_and_links() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: vec![row("f92704a8-5495-4a52-b2e8-56e6fd2d12ee", "السلام عليكم")],
            server_time: "12:00".to_string(),
        };
        let html = inbox(Locale::Ar, &model).into_string();

        assert!(html.contains("السلام عليكم"));
        assert!(html.contains("aicha@uca.ma"));
        assert!(html.contains("yassine@tira.test"));
        assert!(html.contains(
            "/mail/33333333-3333-4333-8333-333333333333/message/f92704a8-5495-4a52-b2e8-56e6fd2d12ee"
        ));
        // Composing is implemented, so "new message" is a real link into the
        // live compose route — not a disabled "coming soon" control. The copy
        // must match the capability.
        assert!(html.contains("href=\"/mail/33333333-3333-4333-8333-333333333333/compose\""));
        assert!(!html.contains("is-soon"));
        assert!(!html.contains("قريبًا"));
        // The Sent folder is unavailable because there is no folder view yet,
        // not because the send path is disconnected (send.rs is implemented).
        assert!(!html.contains("غير موصول"));
        // Unsupported folders are collapsed into one honest section instead of
        // six dashed buttons, and the real folder is marked as current.
        assert!(html.contains("غير متاح بعد"));
        assert!(html.contains("aria-current=\"page\""));
        // And nothing points at a dead anchor.
        assert!(!html.contains("href=\"#\""));
        // The inbox is the split workspace with an empty reader pane. The
        // narrow layout picks a pane from `data-view`, with no JavaScript.
        assert!(html.contains("data-view=\"inbox\""));
        assert!(html.contains("pane--list"));
        assert!(html.contains("pane--reader"));
        assert!(html.contains("reader-empty"));
    }

    #[test]
    fn inbox_shows_a_real_send_confirmation_and_its_limits() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: vec![row("f92704a8-5495-4a52-b2e8-56e6fd2d12ee", "السلام عليكم")],
            server_time: "12:00".to_string(),
        };
        let notice = SentNotice {
            to: "aicha@uca.ma".to_string(),
            subject: "Re: Salam".to_string(),
            accepted_at: "2026-11-17 13:00".to_string(),
            relay: "127.0.0.1:1025".to_string(),
        };
        let html = inbox_with_notice(Locale::Ar, &model, Some(&notice)).into_string();

        assert!(html.contains("role=\"status\""));
        assert!(html.contains("تم إرسال الرسالة"));
        assert!(html.contains("aicha@uca.ma"));
        // The relay endpoint is an implementation detail, not a user fact.
        assert!(!html.contains("127.0.0.1:1025"));
        // The limits are stated, not implied: relay acceptance is not delivery,
        // and no Sent folder exists yet.
        assert!(html.contains("لا يثبت وصولها"));
        assert!(html.contains("مجلد «مُرسَل»"));

        // Without an issued notice there is no banner at all: the query string
        // alone cannot fabricate a confirmation.
        let plain = inbox(Locale::Ar, &model).into_string();
        assert!(!plain.contains("sent-notice"));
    }

    #[test]
    fn the_sidebar_does_not_claim_a_sync_that_does_not_exist() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: vec![row("f92704a8-5495-4a52-b2e8-56e6fd2d12ee", "السلام عليكم")],
            server_time: "12:00".to_string(),
        };
        let html = inbox(Locale::Ar, &model).into_string();

        assert!(!html.contains("مزامنة آمنة"));
        assert!(!html.contains("جلسة آمنة"));
        assert!(html.contains("لا مزامنة مع خدمات خارجية"));
        assert!(html.contains("قابلة للإلغاء من الملف الشخصي"));
    }

    #[test]
    fn split_view_keeps_the_list_and_marks_the_open_message() {
        let reading = reading();
        let html = inbox_with_reader(Locale::Ar, &inbox_for(&reading), &reading).into_string();

        assert!(html.contains("data-view=\"message\""));
        // Both panes are server-rendered in one response: opening a message
        // does not drop the list it was opened from.
        assert!(html.contains("pane--list"));
        assert!(html.contains("pane--reader"));
        assert!(html.contains("Salam Yassine"));
        // The open row is the current one, for the stylesheet and for
        // assistive technology.
        assert!(html.contains("row-link row-link--current"));
        assert!(html.contains("aria-current=\"page\""));
        assert!(!html.contains("reader-empty"));
        assert!(!html.contains("href=\"#\""));
    }

    #[test]
    fn empty_inbox_says_so_instead_of_inventing_rows() {
        let model = InboxModel {
            mailbox: MailboxModel {
                total: 0,
                ..mailbox()
            },
            messages: Vec::new(),
            server_time: "12:00".to_string(),
        };
        let html = inbox(Locale::Ar, &model).into_string();
        assert!(html.contains("لا توجد رسائل بعد"));
        // The only address on an empty page is the mailbox's own, inside the
        // "send to yourself" link - never an invented message row.
        assert!(html.contains("/compose?to="));
        assert!(!html.contains("row-link"));
    }

    #[test]
    fn hostile_subject_is_escaped() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: vec![row("id", "<script>alert(1)</script>")],
            server_time: "12:00".to_string(),
        };
        let html = inbox(Locale::Ar, &model).into_string();
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert"));
    }

    fn reading() -> ReadingModel {
        ReadingModel {
            mailbox: mailbox(),
            server_time: "12:00".to_string(),
            message: ReadingMessage {
                id: "f92704a8-5495-4a52-b2e8-56e6fd2d12ee".to_string(),
                subject: Some("السلام عليكم".to_string()),
                from_addr: "aicha@uca.ma".to_string(),
                to_addr: "yassine@tira.test".to_string(),
                message_id_hdr: Some("live-demo@uca.ma".to_string()),
                size_bytes: 119,
                received_full: "2026-11-04 10:42".to_string(),
                received_iso: "2026-11-04T09:42:00+00:00".to_string(),
                body: Some("Salam Yassine, hada awal imtihan haqiqi.".to_string()),
                authenticity: None,
            },
        }
    }

    fn reading_with(message_auth: Option<MessageAuth>) -> ReadingModel {
        let mut model = reading();
        model.message.authenticity = message_auth;
        model
    }

    fn auth(domain: &str, checks: Vec<CheckView>) -> MessageAuth {
        MessageAuth {
            domain: domain.to_string(),
            checks,
        }
    }

    fn check(id: &'static str, status: &'static str, detail: &str) -> CheckView {
        CheckView {
            id,
            status,
            detail: detail.to_string(),
        }
    }

    /// The inbox half of the split view, built from the same facts as the
    /// reading model so a test exercises one message in both panes.
    fn inbox_for(reading: &ReadingModel) -> InboxModel {
        InboxModel {
            mailbox: reading.mailbox.clone(),
            messages: vec![MessageModel {
                id: reading.message.id.clone(),
                from_addr: reading.message.from_addr.clone(),
                subject: reading.message.subject.clone(),
                snippet: Some("Salam Yassine".to_string()),
                size_bytes: reading.message.size_bytes,
                received_label: "10:42".to_string(),
            }],
            server_time: reading.server_time.clone(),
        }
    }

    #[test]
    fn message_page_shows_body_and_rail() {
        let html = inbox_with_reader(Locale::Ar, &inbox_for(&reading()), &reading()).into_string();
        assert!(html.contains("Salam Yassine"));
        assert!(html.contains("live-demo@uca.ma"));
        assert!(html.contains("/api/v1/messages/f92704a8-5495-4a52-b2e8-56e6fd2d12ee/raw"));
        assert!(html.contains("download=\"message.eml\""));
        assert!(html.contains("عرض HTML"));
        assert!(html.contains("المرفقات — غير متاح بعد"));
        // The reply affordance is live: the reader links into the compose screen.
        assert!(html.contains("/mail/33333333-3333-4333-8333-333333333333/compose?to=aicha@uca.ma"));
        // Professional two-column reading layout: body + sender/metadata rail.
        assert!(html.contains("reader-grid"));
        assert!(html.contains("reader-rail"));
        assert!(html.contains("rail-card__domain"));
        assert!(html.contains("uca.ma"));
        assert!(html.contains("datetime=\"2026-11-04T09:42:00+00:00\""));
        // The print view is a real navigation target, not a scripted dialog.
        assert!(html.contains(
            "/mail/33333333-3333-4333-8333-333333333333/message/f92704a8-5495-4a52-b2e8-56e6fd2d12ee/print"
        ));
        // The old duplicate header disclosure is gone: the rail is the single
        // source of metadata.
        assert!(!html.contains("class=\"headers\""));
        assert!(!html.contains("href=\"#\""));
    }

    #[test]
    fn message_renders_sender_authentication_when_available() {
        let model = reading_with(Some(auth(
            "uca.ma",
            vec![
                check("spf", "pass", "v=spf1 include:_spf.uca.ma ~all"),
                check("dkim.rsa", "pass", "v=DKIM1; rsa1; 2048 bits"),
                check(
                    "dmarc",
                    "pass",
                    "v=DMARC1; p=reject; rua=mailto:dmarc@uca.ma",
                ),
            ],
        )));
        let html = inbox_with_reader(Locale::Ar, &inbox_for(&model), &model).into_string();

        // The card is honest about what it renders: the sender domain's DNS
        // records, not a per-message authentication result.
        assert!(html.contains("سجلات نطاق المرسل"));
        assert!(html.contains("uca.ma"));
        assert!(html.contains("reader-auth__checks"));
        assert!(html.contains("v=spf1 include:_spf.uca.ma ~all"));
        assert!(html.contains("v=DKIM1"));
        assert!(html.contains("p=reject"));
        // Presence wording, never a message-level verdict.
        assert!(html.contains("موجود"));
        assert!(!html.contains("سليم"));
        assert!(!html.contains(">فشل<"));
        assert!(html.contains("لا نتيجة المصادقة لهذه الرسالة"));
    }

    #[test]
    fn a_missing_record_is_not_dressed_up_as_a_pass() {
        let model = reading_with(Some(auth(
            "uca.ma",
            vec![
                check("spf", "fail", "no SPF record in the zone"),
                check("dkim.rsa", "warn", "no RSA selector reaches 2048 bits"),
            ],
        )));
        let html = inbox_with_reader(Locale::Ar, &inbox_for(&model), &model).into_string();

        assert!(html.contains("غير مستوفٍ"));
        assert!(html.contains("تنبيه"));
        assert!(html.contains("no SPF record in the zone"));
        assert!(!html.contains("سليم"));
    }

    #[test]
    fn message_auth_unavailable_states_instead_of_faking_a_verdict() {
        let html = inbox_with_reader(Locale::Ar, &inbox_for(&reading()), &reading()).into_string();

        assert!(html.contains("سجلات نطاق المرسل"));
        assert!(html.contains("تعذّر قراءة سجلات DNS المخزّنة"));
        assert!(!html.contains("reader-auth__check"));
    }

    #[test]
    fn compose_screen_renders_draft_and_error() {
        let model = mailbox();
        let html = compose(
            Locale::Ar,
            &model,
            "aicha@uca.ma",
            "Re: Salam",
            "Salam Yassine, hada awal.",
            None,
        )
        .into_string();

        // The form really posts to the send endpoint, seeded from the reply.
        assert!(html.contains("action=\"/mail/33333333-3333-4333-8333-333333333333/send\""));
        assert!(html.contains("name=\"to\""));
        assert!(html.contains("name=\"subject\""));
        assert!(html.contains("name=\"body\""));
        assert!(html.contains("value=\"aicha@uca.ma\""));
        assert!(html.contains("value=\"Re: Salam\""));
        assert!(html.contains("Salam Yassine, hada awal."));
        assert!(html.contains("تحرير الرسالة"));

        // A `textarea` closed with `;` (void-style) leaves the start tag open,
        // and the browser then parses the submit button as part of the field's
        // text — the form cannot be submitted at all. The draft must sit inside
        // the element and the actions must follow its closing tag.
        let body_at = html.find("name=\"body\"").expect("body field");
        let close_at = html.find("</textarea>").expect("textarea must be closed");
        // The topbar's logout button is also `type="submit"`, so compare against
        // the last one: the compose form's own submit control.
        let submit_at = html.rfind("type=\"submit\"").expect("submit button");
        assert!(body_at < close_at, "draft must be inside the textarea");
        assert!(
            close_at < submit_at,
            "submit button must follow the textarea, not be swallowed by it"
        );
        let inside = &html[body_at..close_at];
        assert!(
            inside.contains("Salam Yassine, hada awal."),
            "draft text must be inside the textarea, got: {inside}"
        );

        let failed = compose(
            Locale::Ar,
            &model,
            "aicha@uca.ma",
            "Re: Salam",
            "",
            Some("جسم الرسالة مطلوب."),
        );
        let failed_html = failed.into_string();
        assert!(failed_html.contains("جسم الرسالة مطلوب."));
        assert!(failed_html.contains("value=\"aicha@uca.ma\""));
        // Nothing points at a dead anchor.
        assert!(!failed_html.contains("href=\"#\""));
    }

    fn profile_model() -> ProfileModel {
        ProfileModel {
            mailbox: mailbox(),
            display_name: Some("ياسين".to_string()),
            address: "yassine@tira.test".to_string(),
            created_label: "2026-11-01 09:00".to_string(),
            message_count: 2,
            password_set: true,
            engine_state: "pending".to_string(),
            sessions: vec![
                SessionModel {
                    id: "11111111-1111-4111-8111-111111111111".to_string(),
                    label: "session:web".to_string(),
                    created_label: "2026-11-08 10:00".to_string(),
                    last_used_label: "2026-11-08 11:00".to_string(),
                    expires_label: "2026-11-15 10:00".to_string(),
                    is_current: true,
                },
                SessionModel {
                    id: "22222222-2222-4222-8222-222222222222".to_string(),
                    label: "session:web".to_string(),
                    created_label: "2026-11-07 10:00".to_string(),
                    last_used_label: "لم تُستخدم بعد".to_string(),
                    expires_label: "2026-11-14 10:00".to_string(),
                    is_current: false,
                },
            ],
            current_token_id: "11111111-1111-4111-8111-111111111111".to_string(),
            status: Some("حُفظ الاسم الظاهر."),
            error: None,
        }
    }

    #[test]
    fn profile_has_real_forms_and_revocable_sessions() {
        let html = profile(Locale::Ar, &profile_model()).into_string();
        assert!(html.contains("الملف الشخصي"));
        assert!(html.contains("ياسين"));
        assert!(html.contains("yassine@tira.test"));
        assert!(html.contains("action=\"/profile/display-name\""));
        assert!(html.contains("action=\"/profile/password\""));
        assert!(html.contains("name=\"current_password\""));
        assert!(html.contains("action=\"/profile/sessions/revoke\""));
        assert!(html.contains("action=\"/profile/sessions/revoke-others\""));
        assert!(html.contains("هذه الجلسة"));
        assert!(html.contains("2026-11-15 10:00"));
        // The current session is not offered a revoke button; the other is.
        assert!(html.contains("name=\"session_id\" value=\"22222222-2222-4222-8222-222222222222\""));
        assert!(!html.contains("11111111-1111-4111-8111-111111111111"));
        assert!(html.contains("أين تعمل هذه الكلمة"));
        assert!(!html.contains("href=\"#\""));
        assert!(!html.contains("https://"));
        // The profile speaks about the person's account, not about the
        // infrastructure behind it.
        for term in [
            "Stalwart",
            "IMAP",
            "Argon2id",
            "PostgreSQL",
            "RLS",
            "SHA-256",
        ] {
            assert!(!html.contains(term), "profile leaks {term}");
        }
    }

    #[test]
    fn profile_without_a_password_offers_to_set_one() {
        let mut model = profile_model();
        model.password_set = false;
        model.display_name = None;
        let html = profile(Locale::Ar, &model).into_string();
        assert!(html.contains("لا توجد كلمة مرور بعد"));
        assert!(html.contains("تعيين كلمة المرور"));
        assert!(!html.contains("name=\"current_password\""));
        // No stored label: the local part is the honest fallback.
        assert!(html.contains("yassine"));
    }

    #[test]
    fn print_page_is_paper_first_and_reuses_the_same_facts() {
        let html = message_print(Locale::Ar, &reading()).into_string();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("dir=\"rtl\""));
        assert!(html.contains("print-sheet"));
        assert!(html.contains("Salam Yassine"));
        assert!(html.contains("aicha@uca.ma"));
        assert!(html.contains("yassine@tira.test"));
        assert!(html.contains("2026-11-04 10:42"));
        assert!(html.contains("رجوع إلى الرسالة"));
        assert!(html.contains("Ctrl+P"));
        // Same plain-text rule as the screen page: no images, no HTML body.
        assert!(!html.contains("<img"));
    }

    #[test]
    fn language_switcher_is_relative_and_marks_the_current_locale() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: Vec::new(),
            server_time: "12:00".to_string(),
        };
        let html = inbox(Locale::Fr, &model).into_string();

        // Relative links: they keep the current path and work without script.
        assert!(html.contains("href=\"?lang=ar\""));
        assert!(html.contains("href=\"?lang=fr\""));
        assert!(html.contains("href=\"?lang=en\""));
        assert!(html.contains("hreflang=\"fr\""));
        // The current choice is announced, not just coloured.
        assert!(html.contains("lang__item--active"));
        assert!(html.contains("aria-current=\"true\""));
        // Every choice keeps its own name, whatever the page language.
        assert!(html.contains("العربية"));
        assert!(html.contains("Français"));
        assert!(html.contains("English"));
    }

    #[test]
    fn french_shell_translates_chrome_and_keeps_the_message_text() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: vec![row("f92704a8-5495-4a52-b2e8-56e6fd2d12ee", "السلام عليكم")],
            server_time: "12:00".to_string(),
        };
        let html = inbox(Locale::Fr, &model).into_string();

        assert!(html.contains("lang=\"fr\""));
        assert!(html.contains("dir=\"ltr\""));
        assert!(html.contains("Boîte de réception"));
        assert!(html.contains("Nouveau message"));
        assert!(html.contains("Actualiser"));
        assert!(html.contains("2 messages"));
        assert!(html.contains("Aucune synchronisation avec des services externes"));
        // The sender's words are not translated, even in a French shell.
        assert!(html.contains("السلام عليكم"));
        assert!(html.contains("aicha@uca.ma"));
        // And no Arabic chrome leaked into the French page.
        assert!(!html.contains("رسالة جديدة"));
        assert!(!html.contains("لا مزامنة"));
    }

    #[test]
    fn english_shell_is_free_of_arabic_chrome() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: Vec::new(),
            server_time: "12:00".to_string(),
        };
        let html = inbox(Locale::En, &model).into_string();

        assert!(html.contains("lang=\"en\""));
        assert!(html.contains("Inbox"));
        assert!(html.contains("New message"));
        assert!(html.contains("No messages yet"));
        assert!(html.contains("Send a message to yourself"));
        assert!(html.contains("No demo data in this mailbox."));
        // The page's own chrome must not contain Arabic words; the only Arabic
        // that may appear is content, and this fixture has none.
        for arabic in ["الوارد", "رسالة جديدة", "لا توجد رسائل", "تحديث"]
        {
            assert!(!html.contains(arabic), "English page leaked {arabic}");
        }
    }

    #[test]
    fn content_fields_use_direction_auto_not_a_forced_direction() {
        let html = inbox_with_reader(Locale::Ar, &inbox_for(&reading()), &reading()).into_string();
        // Subject and body are content: each string picks its own direction.
        assert!(html.contains("class=\"reader__subject\" id=\"message-subject\" dir=\"auto\""));
        assert!(html.contains("class=\"reader__body\" dir=\"auto\""));
        // Addresses stay explicitly LTR.
        assert!(html.contains("class=\"reader__to-addr\" dir=\"ltr\""));

        let compose_html = compose(
            Locale::Ar,
            &mailbox(),
            "aicha@uca.ma",
            "Re: Salam",
            "body",
            None,
        )
        .into_string();
        assert!(compose_html.contains("name=\"subject\" value=\"Re: Salam\" dir=\"auto\""));
        assert!(!compose_html.contains("name=\"subject\" value=\"Re: Salam\" dir=\"ltr\""));
    }

    #[test]
    fn profile_is_jargon_free_in_every_locale() {
        for locale in Locale::ALL {
            let html = profile(locale, &profile_model()).into_string();
            for term in [
                "Stalwart",
                "IMAP",
                "Argon2id",
                "PostgreSQL",
                "RLS",
                "SHA-256",
            ] {
                assert!(!html.contains(term), "{:?} profile leaks {term}", locale);
            }
        }
    }
}
