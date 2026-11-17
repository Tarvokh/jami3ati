//! Page-level composition.
//!
//! `inbox`, `inbox_with_reader` and `message_print` live in
//! [`crate::components`] because they share the sidebar and row components;
//! this module owns the screens that do not use the mailbox shell (login,
//! service errors).

use maud::{html, Markup};

use crate::icons::Icon;
use crate::layout::document;

pub use crate::components::{
    compose, inbox, inbox_with_notice, inbox_with_reader, message_print, profile, CheckView,
    MessageAuth, SentNotice,
};
pub use crate::components::{
    InboxModel, MailboxModel, MessageModel, ProfileModel, ReadingModel, SessionModel,
};

/// Login with the local Tira password, with token login kept as the
/// bootstrap/recovery door.
///
/// The password is a **Tira control-plane** credential, not a Stalwart/IMAP
/// password. The copy says that explicitly, because a person who types their
/// mail-engine password here and gets "wrong" would rightly call the product
/// dishonest. The token form remains because it is the only way to set the
/// first password.
pub fn login(error: Option<&str>) -> Markup {
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                div.brand {
                    span.brand__mark { (maud::PreEscaped(crate::icons::Icon::Mail.svg())) }
                    span.brand__text {
                        span.brand__name { "Tira Mail" }
                        span.brand__org { "TIRA LABS" }
                    }
                }

                section.auth__card aria-labelledby="login-title" {
                    span.auth__eyebrow { "دخول آمن · Argon2id" }
                    h1 id="login-title" { "الدخول إلى الصندوق" }
                    p.muted {
                        "سجّل الدخول ببريدك وكلمة المرور الخاصة بـ Tira. "
                        "هذه كلمة مرور واجهة Tira فقط، وليست كلمة مرور Stalwart أو IMAP."
                    }

                    @if let Some(message) = error {
                        div.auth__error role="alert" {
                            span.i { (maud::PreEscaped(Icon::Lock.svg())) }
                            span { (message) }
                        }
                    }

                    form method="post" action="/login" hx-boost="false" {
                        input type="hidden" name="mode" value="password";
                        div.field {
                            label for="email" { "البريد الإلكتروني" }
                            input id="email" name="email" type="email"
                                required autofocus autocomplete="username"
                                spellcheck="false" placeholder="you@tira.test"
                                dir="ltr";
                        }
                        div.field {
                            label for="password" { "كلمة المرور" }
                            input id="password" name="password" type="password"
                                required autocomplete="current-password"
                                placeholder="••••••••••••";
                        }
                        button class="btn btn--primary btn--block" type="submit" {
                            "دخول"
                        }
                    }

                    details.auth__help {
                        summary { "الدخول بتوكن وصول (للمشغّل أو الاستعادة)" }
                        p {
                            "إن لم تضبط كلمة مرور بعد، استخدم توكنًا صادرًا عن "
                            code { "tira-token" }
                            " ثم اضبط كلمة المرور من الملف الشخصي. "
                            "هذا مسار التهيئة والاستعادة، وليس كلمة المرور اليومية."
                        }
                        form method="post" action="/login" hx-boost="false" {
                            input type="hidden" name="mode" value="token";
                            div.field {
                                label for="token" { "توكن الوصول" }
                                input id="token" name="token" type="password"
                                    autocomplete="off" spellcheck="false"
                                    placeholder="tira_…" dir="ltr";
                                span.field__hint {
                                    "يبدأ بـ "
                                    code { "tira_" }
                                    " ولا يمكن استرجاعه بعد إنشائه."
                                }
                            }
                            button class="btn btn--ghost btn--block" type="submit" {
                                "دخول بالتوكن"
                            }
                        }
                    }

                    div.auth__foot {
                        "الجلسة كوكي "
                        code { "tira_token" }
                        " من نوع HttpOnly + SameSite=Strict لمدة 7 أيام. "
                        "كلمات المرور تُخزَّن كبصمة Argon2id، والتوكنات كـ SHA-256؛ "
                        "لا شيء منهما في JavaScript."
                    }
                }

                p.auth__meta { "TIRA LABS · لا موارد خارجية · RLS مفعّل" }
            }
        }
    };

    document("تسجيل الدخول", body)
}

/// Honest terminal state for an infrastructure failure: the process is alive
/// but cannot verify or read anything.
pub fn service_unavailable(detail: &str) -> Markup {
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                section.auth__pitch {
                    span.auth__eyebrow { "خطأ في البنية" }
                    h1.auth__title { "التخزين غير متاح مؤقتًا." }
                    p.auth__lead {
                        "الخادم يعمل لكنه لا يستطيع التحقق من التوكن أو قراءة الرسائل. "
                        "هذا ليس 404: أعد المحاولة بعد لحظات."
                    }
                }
                section.auth__card {
                    div.notice role="alert" {
                        span.i { (maud::PreEscaped(Icon::Info.svg())) }
                        span.mono dir="ltr" { (detail) }
                    }
                    a class="btn btn--ghost btn--block" href="/login" { "إعادة المحاولة" }
                }
            }
        }
    };

    document("التخزين غير متاح", body)
}

/// CSRF boundary failure: the browser sent a state-changing request from an
/// origin the server cannot trust (cross-site, or an opaque `Origin: null`).
/// Deliberately says nothing about accounts or data — only that the request's
/// origin was rejected and no action ran.
pub fn cross_origin_rejected() -> Markup {
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                section.auth__pitch {
                    span.auth__eyebrow { "حماية CSRF" }
                    h1.auth__title { "أصل الطلب غير موثوق." }
                    p.auth__lead {
                        "رُفض هذا الطلب لأن المتصفح أرسله من أصل مختلف أو غير قابل للتحقق. "
                        "هذه حماية من تزوير الطلبات عبر المواقع، ولم تُنفَّذ أي عملية."
                    }
                }
                section.auth__card {
                    div.notice role="alert" {
                        span.i { (maud::PreEscaped(Icon::Lock.svg())) }
                        span {
                            "لم يتغيّر أي شيء. أعد المحاولة من صفحة الدخول داخل هذا الموقع، "
                            "أو حدّث الصفحة إن كنت تستخدم متصفحًا مضمّنًا."
                        }
                    }
                    a class="btn btn--ghost btn--block" href="/login" { "العودة إلى تسجيل الدخول" }
                }
            }
        }
    };

    document("أصل الطلب غير موثوق", body)
}

/// Generic honest failure page. Used for malformed ids, missing mailboxes and
/// any state where saying "not found" is the correct, non-enumerating answer.
pub fn problem(eyebrow: &str, title: &str, message: &str) -> Markup {
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
                            "لم يُعرَض أي محتوى. هذا الرد لا يكشف إن كان المعرّف موجودًا لدى مستأجر آخر."
                        }
                    }
                    a class="btn btn--ghost btn--block" href="/login" { "العودة إلى تسجيل الدخول" }
                }
            }
        }
    };

    document(title, body)
}

/// A tenant-wide token is valid but cannot address one mailbox in a UI that
/// has no mailbox picker yet. The page says that instead of guessing a mailbox.
pub fn scope_notice() -> Markup {
    let body = html! {
        main.auth id="main" {
            div.auth__inner {
                section.auth__pitch {
                    span.auth__eyebrow { "توكن على مستوى المستأجر" }
                    h1.auth__title { "هذا التوكن لا يحدّد صندوقًا واحدًا." }
                    p.auth__lead {
                        "واجهة القراءة تحتاج توكنًا مرتبطًا بصندوق بريد واحد حتى تعرف أي صندوق تعرض. "
                        "واجهة JSON تعمل بالتوكن الحالي كما هو."
                    }
                }
                section.auth__card {
                    div.notice.notice--info role="note" {
                        span.i { (maud::PreEscaped(Icon::KeyRound.svg())) }
                        span {
                            "أصدر توكنًا مرتبطًا بصندوق البريد المطلوب، ثم سجّل الدخول به من جديد. "
                            "لا يوجد في هذا النموذج اختيار صندوق من قائمة."
                        }
                    }
                    form method="post" action="/logout" hx-boost="false" {
                        button class="btn btn--ghost btn--block" type="submit" { "تسجيل الخروج" }
                    }
                }
            }
        }
    };

    document("توكن على مستوى المستأجر", body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_offers_password_first_and_token_as_recovery() {
        let html = login(None).into_string();
        assert!(html.contains("الدخول إلى الصندوق"));
        assert!(html.contains("name=\"email\""));
        assert!(html.contains("name=\"password\""));
        assert!(html.contains("autocomplete=\"current-password\""));
        assert!(html.contains("name=\"token\""));
        assert!(html.contains("mode\" value=\"token\""));
        assert!(html.contains("HttpOnly"));
        // The credential is local to Tira; the page must not imply Stalwart.
        assert!(html.contains("ليست كلمة مرور Stalwart"));
        assert!(!html.contains("غير موصول بعد"));
        assert!(!html.contains("href=\"#\""));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn login_error_is_visible_and_escaped() {
        let html = login(Some("<bad token>")).into_string();
        assert!(html.contains("&lt;bad token&gt;"));
        assert!(html.contains("role=\"alert\""));
    }
}
