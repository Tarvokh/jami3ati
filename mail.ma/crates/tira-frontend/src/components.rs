//! Typed view models and the components that render them.
//!
//! A page is a pure function of its model. The API layer fills the model from
//! the store; nothing here reads a database, a clock, or an environment
//! variable, which is what makes the rendering testable without fixtures.

use maud::{html, Markup, PreEscaped};

use crate::format;
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
    /// `pending`, `active`, `failed`, or `disabled` — the Stalwart bridge state.
    pub engine_state: String,
    pub sessions: Vec<SessionModel>,
    pub current_token_id: String,
    pub status: Option<&'static str>,
    pub error: Option<&'static str>,
}

fn icon(i: Icon) -> Markup {
    html! { span.i { (PreEscaped(i.svg())) } }
}

/// Arabic label for a check status token, with the English token kept in the
/// `data-status` value so the raw fact stays inspectable.
///
/// These checks read the sender domain's *published DNS policy records*, not
/// the authentication result of this particular message. The labels therefore
/// say whether a record is present and satisfactory, never that the message
/// itself "passed": `pass` → «موجود», `fail` → «غير مستوفٍ». A `fail` is still
/// always shown as failed, never softened.
fn check_status_label(status: &str) -> &'static str {
    match status {
        "pass" => "موجود",
        "warn" => "تنبيه",
        "fail" => "غير مستوفٍ",
        _ => "غير معلوم",
    }
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

fn brand(mailbox: Option<&MailboxModel>) -> Markup {
    match mailbox {
        Some(m) => html! {
            a.brand href=(format!("/mail/{}", m.id)) title="الوارد" {
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

fn topbar(mailbox: Option<&MailboxModel>) -> Markup {
    html! {
        header.topbar {
            (brand(mailbox))

            div.topbar__actions {
                @if let Some(m) = mailbox {
                    a class="chip chip--account" href="/profile"
                        title=(format!("الملف الشخصي — {}", m.address)) {
                        (avatar(&m.address, false))
                        span { (m.display_name.as_deref().unwrap_or_else(|| format::local_part(&m.address))) }
                    }
                }
                @if mailbox.is_some() {
                    form method="post" action="/logout" hx-boost="false" {
                        button type="submit" class="icon-btn" aria-label="تسجيل الخروج"
                            title="حذف كوكي الجلسة من هذا المتصفح" {
                            (icon(Icon::LogOut))
                        }
                    }
                }
            }
        }
    }
}

fn sidebar(mailbox: &MailboxModel, active: Section) -> Markup {
    let inbox_active = matches!(active, Section::Inbox | Section::Message);

    html! {
        aside.sidebar aria-label="التنقّل" {
            a class="btn btn--block" href=(format!("/mail/{}/compose", mailbox.id))
                title="تحرير رسالة جديدة من هذا الصندوق" {
                (icon(Icon::Send))
                span { "رسالة جديدة" }
            }

            nav.folders aria-label="المجلدات" {
                a class=(if inbox_active { "folder folder--active" } else { "folder" })
                    href=(format!("/mail/{}", mailbox.id))
                    aria-current=[inbox_active.then_some("page")]
                    title="الوارد — الرسائل المخزّنة في هذا الصندوق" {
                    (icon(Icon::Inbox))
                    span { "الوارد" }
                    span.folder__count { (mailbox.total) }
                }
                a class=(if active == Section::Profile { "folder folder--active" } else { "folder" })
                    href="/profile"
                    aria-current=[(active == Section::Profile).then_some("page")]
                    title="الملف الشخصي — الاسم وكلمة المرور والجلسات" {
                    (icon(Icon::User))
                    span { "الملف الشخصي" }
                }
            }

            // Everything the schema or Stalwart cannot support yet lives in one
            // collapsed, honest list instead of six dashed buttons shouting
            // "قريبًا" across the screen.
            details.unavailable {
                summary {
                    span { "غير متاح بعد" }
                    span.unavailable__chev aria-hidden="true" {}
                }
                ul {
                    li { span { "بحث في البريد" } span { "لا فهرس بحث في المخطط" } }
                    li { span { "المميّز" } span { "لا عمود تصنيف في المخطط" } }
                    li { span { "المُرسَل" } span { "لا عرض لمجلد المُرسَل بعد" } }
                    li { span { "المؤرشف" } span { "لا مجلدات في المخطط" } }
                    li { span { "المهملات" } span { "الحذف غير منفّذ" } }
                }
            }

            div.quota {
                div.quota__head {
                    span { "المساحة" }
                    strong {
                        (format::bytes(mailbox.used_bytes))
                        " / "
                        (format::bytes(mailbox.quota_bytes))
                    }
                }
                progress
                    value=(mailbox.used_bytes.max(0))
                    max=(mailbox.quota_bytes.max(1))
                    aria-label=(format!(
                        "المساحة المستخدمة {:.0} بالمئة",
                        format::pct(mailbox.used_bytes, mailbox.quota_bytes)
                    )) {}
            }

            div.sidebar__foot {
                div.row { (icon(Icon::Database)) span { "لا مزامنة خارجية — البريد يُستقبل عبر SMTP" } }
                div.row { (icon(Icon::Shield)) span { "الجلسة قابلة للإلغاء من الملف الشخصي" } }
            }
        }
    }
}

fn app_shell(title: &str, mailbox: &MailboxModel, active: Section, content: Markup) -> Markup {
    document(
        title,
        html! {
            (topbar(Some(mailbox)))
            div.workspace {
                (sidebar(mailbox, active))
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
    title: &str,
    mailbox: &MailboxModel,
    active: Section,
    view: &str,
    list: Markup,
    reader: Markup,
) -> Markup {
    document(
        title,
        html! {
            (topbar(Some(mailbox)))
            div class="workspace workspace--split" data-view=(view) {
                (sidebar(mailbox, active))
                main.panes id="main" {
                    div.pane.pane--list { (list) }
                    section.pane.pane--reader aria-label="قارئ الرسالة" { (reader) }
                }
            }
        },
    )
}

fn empty_state(title: &str, body: &str) -> Markup {
    html! {
        div.empty {
            (icon(Icon::Inbox))
            h2 { (title) }
            p { (body) }
        }
    }
}

/// The reply/compose screen. This is the target of the `POST /mail/{id}/send`
/// form: the human owns the words, the server owns the envelope (sender = the
/// authenticated mailbox, subject defaults to `Re:`).
///
/// `error` carries a server-side refusal (validation, throttle, upstream). The
/// draft is preserved so a human does not retype a long Arabic body after a
/// failed attempt.
pub fn compose(
    mailbox: &MailboxModel,
    to: &str,
    subject: &str,
    body: &str,
    error: Option<&str>,
) -> Markup {
    let action = format!("/mail/{}/send", mailbox.id);
    let cancel = format!("/mail/{}", mailbox.id);

    let content = html! {
        div.section-head {
            h1 { "تحرير الرسالة" }
            span.section-head__meta {
                "المرسل: "
                span.mono { (mailbox.address) }
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
                    label for="compose-to" { "البريد الإلكتروني" }
                    input id="compose-to" name="to" type="email" required
                        value=(to) autocomplete="email" spellcheck="false" dir="ltr";
                }
                div.field {
                    label for="compose-subject" { "الموضوع" }
                    input id="compose-subject" name="subject" value=(subject) dir="ltr";
                }
                div.field {
                    label for="compose-body" { "النص" }
                    // `textarea` is not a void element: terminating it with `;`
                    // emits an unclosed start tag, and every following node —
                    // including the submit button — is swallowed into the
                    // field's text. The draft goes *inside* the element.
                    textarea id="compose-body" name="body" rows="10" required
                        spellcheck="false" { (body) }
                }
                div.form-actions {
                    button class="btn btn--primary" type="submit" { "إرسال" }
                    a class="btn btn--ghost" href=(cancel) { "إلغاء" }
                }
            }
        }

        p.muted {
            "المرسل هو صاحب الصندوق الحالي؛ وكلمة «البريد الإلكتروني» هي المستلم."
        }
    };

    app_shell("تحرير الرسالة", mailbox, Section::Message, content)
}

/// The list half of the workspace. `selected` marks the row whose message is
/// open in the reader pane, so the current row is announced with
/// `aria-current="page"` instead of looking exactly like its neighbours.
/// `sent` is the one-shot SMTP-handoff confirmation, when the URL carried one.
fn inbox_list(model: &InboxModel, selected: Option<&str>, sent: Option<&SentNotice>) -> Markup {
    let reload = format!("/mail/{}", model.mailbox.id);

    html! {
        div.section-head {
            h1 { "الوارد" }
            span.section-head__meta {
                (model.mailbox.total)
                " رسالة · آخر تحديث "
                span.mono { (model.server_time) }
            }
            div.section-head__actions {
                a class="btn btn--ghost" href=(reload)
                    title="إعادة تحميل القائمة من قاعدة البيانات" {
                    (icon(Icon::Refresh))
                    "تحديث"
                }
            }
        }

        @if let Some(notice) = sent {
            div.sent-notice role="status" {
                div.sent-notice__head {
                    (icon(Icon::Send))
                    strong { "تم تسليم الرسالة إلى مرحّل SMTP" }
                }
                dl.sent-notice__facts {
                    div { dt { "إلى" } dd dir="ltr" { (notice.to) } }
                    div { dt { "الموضوع" } dd { (notice.subject) } }
                    div { dt { "وقت التسليم" } dd { (notice.accepted_at) } }
                    div { dt { "المرحّل" } dd.mono dir="ltr" { (notice.relay) } }
                }
                p.sent-notice__note {
                    "قبِل المرحّل الرسالة (250). هذا ليس إثبات وصول إلى صندوق "
                    "المستلم، ولا توجد نسخة في مجلد «مُرسَل»: المخطط الحالي لا "
                    "يحفظ نسخة مرسلة."
                }
            }
        }

        @if model.messages.is_empty() {
            div.panel {
                (empty_state(
                    "لا توجد رسائل بعد",
                    "هذا الصندوق فارغ في قاعدة البيانات. أرسل رسالة إلى هذا العنوان ثم أعد التحميل."
                ))
            }
        } @else {
            div.panel {
                div.list {
                    @for message in &model.messages {
                        (message_row(
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
            span { "لا توجد بيانات تجريبية في هذا الصندوق." }
        }
    }
}

/// The empty half of the reader pane on the inbox screen. It is a layout
/// affordance, not data: it names the gesture the list already supports.
fn reader_placeholder() -> Markup {
    html! {
        div.reader-empty {
            (icon(Icon::Inbox))
            h2 { "اختر رسالة" }
            p { "اختر رسالة من القائمة لعرض نصّها في هذه المساحة." }
        }
    }
}

pub fn inbox(model: &InboxModel) -> Markup {
    inbox_with_notice(model, None)
}

/// The inbox with a one-shot SMTP-handoff confirmation above the list.
///
/// The notice is issued by the send handler and looked up by an unguessable id
/// bound to the mailbox. It is rendered from server facts only; the template
/// never says the message reached the recipient, and it says explicitly that no
/// Sent folder exists yet.
pub fn inbox_with_notice(model: &InboxModel, sent: Option<&SentNotice>) -> Markup {
    shell_split(
        "الوارد",
        &model.mailbox,
        Section::Inbox,
        "inbox",
        inbox_list(model, None, sent),
        reader_placeholder(),
    )
}

/// The inbox with one message open beside it. The list is the same list the
/// inbox route renders, with the open row marked as current; the reader is the
/// same reading view the message route has always rendered.
pub fn inbox_with_reader(inbox: &InboxModel, reading: &ReadingModel) -> Markup {
    let subject = reading.message.subject.as_deref().unwrap_or("(بلا موضوع)");
    shell_split(
        subject,
        &inbox.mailbox,
        Section::Message,
        "message",
        inbox_list(inbox, Some(reading.message.id.as_str()), None),
        reader(reading),
    )
}

fn message_row(mailbox_id: &str, message: &MessageModel, is_current: bool) -> Markup {
    let href = format!("/mail/{mailbox_id}/message/{}", message.id);
    let subject = message.subject.as_deref().unwrap_or("(بلا موضوع)");

    html! {
        a class=(if is_current { "row-link row-link--current" } else { "row-link" })
            href=(href)
            aria-current=[is_current.then_some("page")] {
            (avatar(&message.from_addr, false))
            div.row-main {
                div.row-top {
                    span.row-from { (format::local_part(&message.from_addr)) }
                    span.row-addr { (message.from_addr) }
                }
                div.row-bottom {
                    span.row-subject { (subject) }
                    @if let Some(snippet) = &message.snippet {
                        span.row-snippet { (snippet) }
                    }
                }
            }
            div.row-side {
                span.row-time { (message.received_label) }
                span.row-size { (format::bytes(message.size_bytes)) }
            }
        }
    }
}

fn reader(model: &ReadingModel) -> Markup {
    let back = format!("/mail/{}", model.mailbox.id);
    let raw = format!("/api/v1/messages/{}/raw", model.message.id);
    let print = format!(
        "/mail/{}/message/{}/print",
        model.mailbox.id, model.message.id
    );
    let subject = model.message.subject.as_deref().unwrap_or("(بلا موضوع)");
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
            header.reader-bar aria-label="أدوات الرسالة" {
                a.back-link href=(back) title="العودة إلى قائمة الوارد" {
                    (icon(Icon::ArrowRight))
                    "الوارد"
                }
                div.reader-bar__context {
                    span.reader-bar__label { "الرسالة" }
                    span.reader-bar__subject { (subject) }
                }
                div.reader-bar__actions {
                    a class="btn btn--ghost reader-bar__btn" href=(print)
                        aria-label="نسخة للطباعة"
                        title="نسخة مهيّأة للورق — تُفتح في صفحة مستقلة" {
                        (icon(Icon::Printer))
                        span.reader-bar__btn-text { "نسخة للطباعة" }
                    }
                    a class="btn btn--ghost reader-bar__btn" href=(raw) download="message.eml"
                        aria-label="تنزيل .eml"
                        title="تنزيل البايتات الأصلية كما استُقبلت عبر SMTP" {
                        (icon(Icon::Download))
                        span.reader-bar__btn-text { "تنزيل .eml" }
                    }
                    a class="btn btn--ghost reader-bar__btn" href=(reply_href)
                        aria-label="الرد"
                        title="الرد إلى المرسل في صندوقك" {
                        (icon(Icon::Reply))
                        span.reader-bar__btn-text { "الرد" }
                    }
                }
            }

            div.reader-grid {
                article.panel.reader-main aria-labelledby="message-subject" {
                    h1.reader__subject id="message-subject" { (subject) }

                    p.reader__to {
                        span.reader__to-label { "إلى" }
                        span.reader__to-addr dir="ltr" { (model.message.to_addr) }
                    }

                    @if let Some(body) = &model.message.body {
                        div.reader__body { (body) }
                    } @else {
                        div.reader__body.muted {
                            "تعذّر استخراج جسم نصّي من هذه الرسالة (قد تكون HTML فقط أو غير مكتملة)."
                        }
                    }

                    p.article__note role="note" {
                        (icon(Icon::Lock))
                        span {
                            "عرض HTML والصور والمرفقات معطّل: التصيير الآمن غير منفّذ بعد، "
                            "فيُعرض النص فقط ولا تُحمَّل أي صورة خارجية."
                        }
                    }

                    details.unavailable {
                        summary {
                            span { "المرفقات — غير متاح بعد" }
                            span.unavailable__chev aria-hidden="true" {}
                        }
                        ul {
                            li { span { "المرفقات" } span { "دعم المرفقات غير مربوط بعد" } }
                        }
                    }
                }

                aside.reader-rail aria-label="بيانات الرسالة" {
                    section.panel.rail-card {
                        header.rail-card__head {
                            (avatar(&model.message.from_addr, true))
                            div.rail-card__who {
                                span.rail-card__name { (sender) }
                                span.rail-card__addr dir="ltr" { (model.message.from_addr) }
                                @if !domain.is_empty() {
                                    span.rail-card__domain dir="ltr" { (domain) }
                                }
                            }
                        }

                        dl.rail-meta {
                            div {
                                dt { "تاريخ الاستلام" }
                                dd {
                                    time datetime=(model.message.received_iso) {
                                        (model.message.received_full)
                                    }
                                }
                            }
                            div {
                                dt { "الحجم" }
                                dd { (format::bytes(model.message.size_bytes)) }
                            }
                            @if let Some(message_id) = &model.message.message_id_hdr {
                                div {
                                    dt { "Message-ID" }
                                    dd.mono dir="ltr" { (message_id) }
                                }
                            } @else {
                                div {
                                    dt { "Message-ID" }
                                    dd.muted { "غير موجود في الترويسة" }
                                }
                            }
                            div {
                                dt { "المعرّف" }
                                dd.mono dir="ltr" { (model.message.id) }
                            }
                        }

                    @if let Some(auth) = &model.message.authenticity {
                        section.panel.rail-card.reader-auth {
                            header.rail-card__head {
                                (icon(Icon::Shield))
                                div.rail-card__who {
                                    span.rail-card__name { "سجلات نطاق المرسل" }
                                    span.rail-card__addr dir="ltr" { (auth.domain) }
                                }
                            }
                            p.reader-auth__scope {
                                "فحوص سجلات DNS المنشورة لهذا النطاق، لا نتيجة "
                                "المصادقة لهذه الرسالة: لا يُفحص توقيع DKIM ولا نتيجة "
                                "SPF/DMARC الفعلية عند الاستلام."
                            }
                            ul.reader-auth__checks {
                                @for check in &auth.checks {
                                    li class=(format!("reader-auth__check reader-auth__check--{}", check.status)) {
                                        span.reader-auth__check-id { (check.id) }
                                        span.reader-auth__check-status { (check_status_label(check.status)) }
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
                                    span.rail-card__name { "سجلات نطاق المرسل" }
                                    span.rail-card__addr dir="ltr" { (domain) }
                                }
                            }
                            p.reader-auth__missing {
                                "تعذّر قراءة سجلات DNS المخزّنة لهذا النطاق؛ "
                                "لا يُعرض أي حكم بدلًا من ذلك."
                            }
                        }
                    }

                        p.rail-note {
                            (icon(Icon::Database))
                            span {
                                "هذه الرسالة مخصصة لهذا الصندوق — لا بيانات تجريبية."
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
pub fn message_print(model: &ReadingModel) -> Markup {
    let back = format!("/mail/{}/message/{}", model.mailbox.id, model.message.id);
    let subject = model.message.subject.as_deref().unwrap_or("(بلا موضوع)");

    document(
        subject,
        html! {
            div.print-page {
                div.print-bar {
                    a.back-link href=(back) title="العودة إلى عرض الرسالة" {
                        (icon(Icon::ArrowRight))
                        "رجوع إلى الرسالة"
                    }
                    p.print-bar__hint {
                        "مهيّأة للورق: استخدم "
                        span.mono dir="ltr" { "Ctrl+P" }
                        " أو "
                        span.mono dir="ltr" { "⌘P" }
                        ". لا صور ولا HTML."
                    }
                }

                article.print-sheet aria-labelledby="print-subject" {
                    header.print-sheet__head {
                        h1 id="print-subject" { (subject) }
                        dl.print-meta {
                            div {
                                dt { "من" }
                                dd dir="ltr" { (model.message.from_addr) }
                            }
                            div {
                                dt { "إلى" }
                                dd dir="ltr" { (model.message.to_addr) }
                            }
                            div {
                                dt { "التاريخ" }
                                dd {
                                    time datetime=(model.message.received_iso) {
                                        (model.message.received_full)
                                    }
                                }
                            }
                            div {
                                dt { "الحجم" }
                                dd { (format::bytes(model.message.size_bytes)) }
                            }
                            @if let Some(message_id) = &model.message.message_id_hdr {
                                div {
                                    dt { "Message-ID" }
                                    dd.mono dir="ltr" { (message_id) }
                                }
                            }
                        }
                    }

                    div.print-sheet__body {
                        @if let Some(body) = &model.message.body {
                            (body)
                        } @else {
                            p.muted {
                                "تعذّر استخراج جسم نصّي من هذه الرسالة (قد تكون HTML فقط أو غير مكتملة)."
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
pub fn profile(model: &ProfileModel) -> Markup {
    let label = model
        .display_name
        .as_deref()
        .unwrap_or_else(|| format::local_part(&model.address));
    let password_heading = if model.password_set {
        "تغيير كلمة المرور"
    } else {
        "تعيين كلمة المرور"
    };
    let engine = engine_label(&model.engine_state);
    let other_sessions = model
        .sessions
        .iter()
        .filter(|session| !session.is_current)
        .count();

    let content = html! {
        div.section-head {
            h1 { "الملف الشخصي" }
            span.section-head__meta { "ملف شخصي حقيقي" }
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
                            h2 id="identity-title" { (label) }
                            p.mono dir="ltr" { (model.address) }
                        }
                    }

                    dl.profile-facts {
                        div {
                            dt { "البريد" }
                            dd.mono dir="ltr" { (model.address) }
                        }
                        div {
                            dt { "أُنشئ" }
                            dd { (model.created_label) }
                        }
                        div {
                            dt { "الرسائل" }
                            dd { (model.message_count) }
                        }
                        div {
                            dt { "المساحة" }
                            dd {
                                (format::bytes(model.mailbox.used_bytes))
                                " / "
                                (format::bytes(model.mailbox.quota_bytes))
                            }
                        }
                        div {
                            dt { "حساب Stalwart" }
                            dd { (engine) }
                        }
                        div {
                            dt { "كلمة مرور Tira" }
                            dd {
                                @if model.password_set {
                                    "مضبوطة"
                                } @else {
                                    "غير مضبوطة"
                                }
                            }
                        }
                    }
                }

                section.panel.profile-section aria-labelledby="name-title" {
                    div.profile-section__head {
                        h2 id="name-title" { "الاسم الظاهر" }
                        p {
                            "يظهر في الشريط العلوي وفي قائمة الجلسات. "
                            "لا يغيّر عنوان البريد ولا يُستخدم في الدخول."
                        }
                    }
                    form method="post" action="/profile/display-name" hx-boost="false" {
                        div.field {
                            label for="display_name" { "الاسم" }
                            input id="display_name" name="display_name" type="text"
                                class="input--text"
                                value=(model.display_name.as_deref().unwrap_or(""))
                                maxlength="80" autocomplete="nickname"
                                placeholder="مثال: ياسين";
                            span.field__hint {
                                "اتركه فارغًا للعودة إلى اسم العنوان المحلي."
                            }
                        }
                        button class="btn btn--primary" type="submit" { "حفظ الاسم" }
                    }
                }

                section.panel.profile-section aria-labelledby="password-title" {
                    div.profile-section__head {
                        h2 id="password-title" { (password_heading) }
                        p {
                            "كلمة مرور واجهة Tira، مخزّنة كبصمة Argon2id. "
                            "هذه ليست كلمة مرور Stalwart أو IMAP؛ حساب المحرك منفصل."
                        }
                    }

                    @if !model.password_set {
                        div.notice.notice--info role="note" {
                            span.i { (icon(Icon::Info)) }
                            span {
                                "لا توجد كلمة مرور بعد. عيّن واحدة الآن، وستبقى الجلسة الحالية صالحة."
                            }
                        }
                    }

                    form method="post" action="/profile/password" hx-boost="false" {
                        @if model.password_set {
                            div.field {
                                label for="current_password" { "كلمة المرور الحالية" }
                                input id="current_password" name="current_password"
                                    type="password" autocomplete="current-password"
                                    required;
                            }
                        }
                        div.field {
                            label for="new_password" { "كلمة المرور الجديدة" }
                            input id="new_password" name="new_password"
                                type="password" autocomplete="new-password"
                                required minlength="12" maxlength="128";
                            span.field__hint {
                                "12 حرفًا على الأقل، و128 كحد أقصى. لا تستخدم بريدك."
                            }
                        }
                        div.field {
                            label for="confirm_password" { "تأكيد كلمة المرور" }
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
                        h2 id="sessions-title" { "الجلسات النشطة" }
                        p {
                            "كل صف توكن جلسة مرتبط بهذا الصندوق، مخزّن مُهشَّرًا "
                            "بـ SHA-256. الإلغاء فوري ولا يمكن التراجع عنه."
                        }
                    }

                    @if model.sessions.is_empty() {
                        p.muted { "لا توجد جلسات نشطة." }
                    } @else {
                        ul.session-list {
                            @for session in &model.sessions {
                                li.session {
                                    div.session__main {
                                        div.session__head {
                                            span.session__label { (session.label) }
                                            @if session.is_current {
                                                span.badge { "هذه الجلسة" }
                                            }
                                        }
                                        dl.session__meta {
                                            div {
                                                dt { "أُنشئت" }
                                                dd { (session.created_label) }
                                            }
                                            div {
                                                dt { "آخر استخدام" }
                                                dd { (session.last_used_label) }
                                            }
                                            div {
                                                dt { "تنتهي" }
                                                dd { (session.expires_label) }
                                            }
                                        }
                                    }
                                    @if !session.is_current {
                                        form method="post" action="/profile/sessions/revoke" hx-boost="false" {
                                            input type="hidden" name="session_id" value=(session.id);
                                            button class="btn btn--ghost btn--danger" type="submit" {
                                                "إلغاء"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        @if other_sessions > 0 {
                            form method="post" action="/profile/sessions/revoke-others" hx-boost="false" {
                                button class="btn btn--ghost btn--block" type="submit" {
                                    "إلغاء كل الجلسات الأخرى"
                                }
                            }
                        }
                    }
                }

                section.panel.profile-section {
                    div.profile-section__head {
                        h2 { "نطاق هذه الكلمة" }
                    }
                    p.muted {
                        "كلمة المرور هذه تفتح واجهة Tira فقط. "
                        "بريد Stalwart وIMAP لهما بيانات اعتماد منفصلة، "
                        "ولا يغيّرها هذا النموذج."
                    }
                }
            }
        }

        p.list-note {
            (icon(Icon::Shield))
            span { "ثقة الجلسة تبقى: هذا النموذج لا يخزّن الأسرار." }
        }
    };

    app_shell("الملف الشخصي", &model.mailbox, Section::Profile, content)
}

fn engine_label(state: &str) -> &'static str {
    match state {
        "active" => "نشط — مُهيّأ في المحرك",
        "pending" => "قيد التهيئة",
        "failed" => "فشلت التهيئة",
        "disabled" => "معطّل",
        _ => "حالة غير معروفة",
    }
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
        let html = inbox(&model).into_string();

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
        let html = inbox_with_notice(&model, Some(&notice)).into_string();

        assert!(html.contains("role=\"status\""));
        assert!(html.contains("تم تسليم الرسالة إلى مرحّل SMTP"));
        assert!(html.contains("aicha@uca.ma"));
        assert!(html.contains("127.0.0.1:1025"));
        // The limits are stated, not implied: relay acceptance is not delivery,
        // and no Sent folder exists yet.
        assert!(html.contains("ليس إثبات وصول"));
        assert!(html.contains("مجلد «مُرسَل»"));

        // Without an issued notice there is no banner at all: the query string
        // alone cannot fabricate a confirmation.
        let plain = inbox(&model).into_string();
        assert!(!plain.contains("sent-notice"));
    }

    #[test]
    fn the_sidebar_does_not_claim_a_sync_that_does_not_exist() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: vec![row("f92704a8-5495-4a52-b2e8-56e6fd2d12ee", "السلام عليكم")],
            server_time: "12:00".to_string(),
        };
        let html = inbox(&model).into_string();

        assert!(!html.contains("مزامنة آمنة"));
        assert!(!html.contains("جلسة آمنة"));
        assert!(html.contains("لا مزامنة خارجية"));
        assert!(html.contains("قابلة للإلغاء من الملف الشخصي"));
    }

    #[test]
    fn split_view_keeps_the_list_and_marks_the_open_message() {
        let reading = reading();
        let html = inbox_with_reader(&inbox_for(&reading), &reading).into_string();

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
        let html = inbox(&model).into_string();
        assert!(html.contains("لا توجد رسائل بعد"));
        assert!(!html.contains("aicha@uca.ma"));
    }

    #[test]
    fn hostile_subject_is_escaped() {
        let model = InboxModel {
            mailbox: mailbox(),
            messages: vec![row("id", "<script>alert(1)</script>")],
            server_time: "12:00".to_string(),
        };
        let html = inbox(&model).into_string();
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
        let html = inbox_with_reader(&inbox_for(&reading()), &reading()).into_string();
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
        let html = inbox_with_reader(&inbox_for(&model), &model).into_string();

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
        let html = inbox_with_reader(&inbox_for(&model), &model).into_string();

        assert!(html.contains("غير مستوفٍ"));
        assert!(html.contains("تنبيه"));
        assert!(html.contains("no SPF record in the zone"));
        assert!(!html.contains("سليم"));
    }

    #[test]
    fn message_auth_unavailable_states_instead_of_faking_a_verdict() {
        let html = inbox_with_reader(&inbox_for(&reading()), &reading()).into_string();

        assert!(html.contains("سجلات نطاق المرسل"));
        assert!(html.contains("تعذّر قراءة سجلات DNS المخزّنة"));
        assert!(!html.contains("reader-auth__check"));
    }

    #[test]
    fn compose_screen_renders_draft_and_error() {
        let model = mailbox();
        let html = compose(
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
        let html = profile(&profile_model()).into_string();
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
        assert!(html.contains("نطاق هذه الكلمة"));
        assert!(!html.contains("href=\"#\""));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn profile_without_a_password_offers_to_set_one() {
        let mut model = profile_model();
        model.password_set = false;
        model.display_name = None;
        let html = profile(&model).into_string();
        assert!(html.contains("لا توجد كلمة مرور بعد"));
        assert!(html.contains("تعيين كلمة المرور"));
        assert!(!html.contains("name=\"current_password\""));
        // No stored label: the local part is the honest fallback.
        assert!(html.contains("yassine"));
    }

    #[test]
    fn print_page_is_paper_first_and_reuses_the_same_facts() {
        let html = message_print(&reading()).into_string();
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
}
