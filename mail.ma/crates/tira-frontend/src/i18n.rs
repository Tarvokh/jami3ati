//! Interface languages for the whole product.
//!
//! Three locales ship: Arabic (the product default, RTL), French and English.
//! The strings live in `&'static` tables, so a missing translation is a compile
//! error rather than an Arabic sentence leaking into a French page.
//!
//! Two tables exist because the two surfaces have different vocabulary:
//!
//!   * [`AuthText`] — login and signup, plus the sentences their handlers
//!     return. A person who has not signed in yet.
//!   * [`ShellText`] — the mailbox shell: sidebar, inbox list, reader, compose,
//!     profile, and the failure pages. A person who is already signed in.
//!
//! Numbers and verdict tokens are not string fields: they depend on the value
//! and on the locale's plural rules, so they are methods on [`Locale`]
//! ([`Locale::messages_count`], [`Locale::storage_used`],
//! [`Locale::check_status`], [`Locale::engine_label`]).

/// A shipped interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    Ar,
    Fr,
    En,
}

impl Locale {
    /// Every locale, in switcher order.
    pub const ALL: [Locale; 3] = [Locale::Ar, Locale::Fr, Locale::En];

    /// BCP 47 primary tag, used for `<html lang>`.
    pub fn code(self) -> &'static str {
        match self {
            Locale::Ar => "ar",
            Locale::Fr => "fr",
            Locale::En => "en",
        }
    }

    /// Writing direction for `<html dir>`.
    pub fn dir(self) -> &'static str {
        match self {
            Locale::Ar => "rtl",
            Locale::Fr | Locale::En => "ltr",
        }
    }

    /// The language's own name, as a language switcher should show it.
    pub fn label(self) -> &'static str {
        match self {
            Locale::Ar => "العربية",
            Locale::Fr => "Français",
            Locale::En => "English",
        }
    }

    /// Parses `ar`, `fr`, `en`, and regional forms such as `fr-MA` or `en_US`.
    pub fn from_code(value: &str) -> Option<Locale> {
        let tag = value.trim().to_ascii_lowercase();
        let primary = tag.split(['-', '_']).next().unwrap_or("");
        match primary {
            "ar" => Some(Locale::Ar),
            "fr" => Some(Locale::Fr),
            "en" => Some(Locale::En),
            _ => None,
        }
    }

    /// First supported language in an `Accept-Language` header.
    ///
    /// Quality values are honoured only to the extent of preserving the header's
    /// order, which is what browsers emit for a normal preference list. A header
    /// with explicit `q=` weights is not re-sorted: that would be a guess, and a
    /// wrong guess is worse than falling back to Arabic.
    pub fn from_accept_language(header: &str) -> Option<Locale> {
        header
            .split(',')
            .filter_map(|part| Locale::from_code(part.split(';').next().unwrap_or("")))
            .next()
    }

    /// The authentication strings for this locale.
    pub fn auth(self) -> &'static AuthText {
        match self {
            Locale::Ar => &AR,
            Locale::Fr => &FR,
            Locale::En => &EN,
        }
    }

    /// The mailbox-shell strings for this locale.
    pub fn shell(self) -> &'static ShellText {
        match self {
            Locale::Ar => &SHELL_AR,
            Locale::Fr => &SHELL_FR,
            Locale::En => &SHELL_EN,
        }
    }

    /// The language's own name, shown in the switcher. Never translated.
    pub fn native_name(self) -> &'static str {
        match self {
            Locale::Ar => "العربية",
            Locale::Fr => "Français",
            Locale::En => "English",
        }
    }

    /// The `<meta name="description">` sentence for this locale.
    pub fn meta_description(self) -> &'static str {
        match self {
            Locale::Ar => "Tira Mail — واجهة قراءة البريد الآمنة لـ TIRA LABS",
            Locale::Fr => "Tira Mail — interface de lecture sécurisée pour TIRA LABS",
            Locale::En => "Tira Mail — a secure reading interface by TIRA LABS",
        }
    }

    /// A count of messages with the locale's plural form.
    ///
    /// Arabic has six forms; French and English have two. Writing the numbers
    /// into a template by hand is how "3 رسالة" leaks into a page, so the rule
    /// lives here and is tested.
    pub fn messages_count(self, n: i64) -> String {
        let n = n.max(0);
        match self {
            Locale::Ar => match n {
                0 => "لا رسائل".to_string(),
                1 => "رسالة واحدة".to_string(),
                2 => "رسالتان".to_string(),
                3..=10 => format!("{n} رسائل"),
                _ => format!("{n} رسالة"),
            },
            Locale::Fr => match n {
                0 => "aucun message".to_string(),
                1 => "1 message".to_string(),
                _ => format!("{n} messages"),
            },
            Locale::En => match n {
                0 => "no messages".to_string(),
                1 => "1 message".to_string(),
                _ => format!("{n} messages"),
            },
        }
    }

    /// The `aria-label` of the storage bar, with the percentage already rounded.
    pub fn storage_used(self, pct: u32) -> String {
        match self {
            Locale::Ar => format!("المساحة المستخدمة {pct} بالمئة"),
            Locale::Fr => format!("Espace utilisé : {pct} %"),
            Locale::En => format!("{pct}% of storage used"),
        }
    }

    /// Human label for a DNS-record check token. Presence wording, never a
    /// message-level verdict: `pass` is "present", not "this message passed".
    pub fn check_status(self, status: &str) -> &'static str {
        match (self, status) {
            (Locale::Ar, "pass") => "موجود",
            (Locale::Ar, "warn") => "تنبيه",
            (Locale::Ar, "fail") => "غير مستوفٍ",
            (Locale::Ar, _) => "غير معلوم",
            (Locale::Fr, "pass") => "présent",
            (Locale::Fr, "warn") => "avertissement",
            (Locale::Fr, "fail") => "non conforme",
            (Locale::Fr, _) => "inconnu",
            (Locale::En, "pass") => "present",
            (Locale::En, "warn") => "warning",
            (Locale::En, "fail") => "not satisfied",
            (Locale::En, _) => "unknown",
        }
    }

    /// Human label for the mail-engine bridge state. The state token is the
    /// stored fact; this is only its display name.
    pub fn engine_label(self, state: &str) -> &'static str {
        match (self, state) {
            (Locale::Ar, "active") => "مفعّل",
            (Locale::Ar, "pending") => "قيد التفعيل",
            (Locale::Ar, "failed") => "تعذّر التفعيل",
            (Locale::Ar, "disabled") => "غير مفعّل",
            (Locale::Ar, _) => "غير معروف",
            (Locale::Fr, "active") => "activé",
            (Locale::Fr, "pending") => "activation en cours",
            (Locale::Fr, "failed") => "échec de l'activation",
            (Locale::Fr, "disabled") => "désactivé",
            (Locale::Fr, _) => "inconnu",
            (Locale::En, "active") => "enabled",
            (Locale::En, "pending") => "activating",
            (Locale::En, "failed") => "activation failed",
            (Locale::En, "disabled") => "disabled",
            (Locale::En, _) => "unknown",
        }
    }

    /// Title of the account chip: the profile link names the destination and
    /// the address it belongs to.
    pub fn profile_chip_title(self, address: &str) -> String {
        match self {
            Locale::Ar => format!("الملف الشخصي — {address}"),
            Locale::Fr => format!("Profil — {address}"),
            Locale::En => format!("Profile — {address}"),
        }
    }

    /// The dev-only sentence shown when the local signup domain was never
    /// bootstrapped. Loopback-only page, so naming the script is actionable.
    pub fn local_domain_missing(self, domain: &str) -> String {
        match self {
            Locale::Ar => format!("لم يُهيَّأ النطاق {domain} بعد. شغّل scripts/dev-local.ps1 أولًا."),
            Locale::Fr => {
                format!("Le domaine {domain} n'est pas encore configuré. Lancez scripts/dev-local.ps1 d'abord.")
            }
            Locale::En => {
                format!("Domain {domain} is not set up yet. Run scripts/dev-local.ps1 first.")
            }
        }
    }
}

/// Every sentence the authentication screens and their handlers render.
///
/// Field names describe the *purpose*, never the Arabic wording, so a
/// translator can work without reading the templates.
#[derive(Debug)]
pub struct AuthText {
    // Document titles.
    pub title_login: &'static str,
    pub title_signup: &'static str,

    // Login screen.
    pub heading_login: &'static str,
    pub lead_login: &'static str,
    pub email: &'static str,
    pub password: &'static str,
    pub sign_in: &'static str,
    pub caps_lock: &'static str,
    pub recovery_summary: &'static str,
    pub recovery_body: &'static str,
    pub token_label: &'static str,
    pub token_hint: &'static str,
    pub token_submit: &'static str,
    pub no_mailbox: &'static str,
    pub create_mailbox_link: &'static str,

    // Signup screen.
    pub heading_signup: &'static str,
    pub lead_signup: &'static str,
    pub local_part: &'static str,
    pub local_hint: &'static str,
    pub display_name: &'static str,
    pub optional: &'static str,
    pub password_min: &'static str,
    pub password_hint: &'static str,
    pub confirm: &'static str,
    pub mismatch: &'static str,
    pub create_mailbox: &'static str,
    pub have_mailbox: &'static str,
    pub sign_in_link: &'static str,

    // Shared chrome.
    pub language: &'static str,
    pub demo_note: &'static str,
    pub copyright: &'static str,
    pub skip_to_content: &'static str,
    pub loading: &'static str,
    pub reveal_show: &'static str,
    pub reveal_hide: &'static str,

    // Login errors, rendered by the API handler.
    pub err_bad_email: &'static str,
    pub err_password_required: &'static str,
    pub err_password_long: &'static str,
    pub err_credentials: &'static str,
    pub err_token_required: &'static str,
    pub err_token_invalid: &'static str,
    pub err_session_expired: &'static str,
    pub err_too_many: &'static str,

    // Signup errors, rendered by the API handler.
    pub err_local_invalid: &'static str,
    pub err_display_long: &'static str,
    pub err_password_short: &'static str,
    pub err_password_too_long: &'static str,
    pub err_mismatch: &'static str,
    pub err_taken: &'static str,
}

static AR: AuthText = AuthText {
    title_login: "تسجيل الدخول",
    title_signup: "إنشاء صندوق",

    heading_login: "تسجيل الدخول",
    lead_login: "أدخل بريدك الإلكتروني وكلمة المرور للمتابعة.",
    email: "البريد الإلكتروني",
    password: "كلمة المرور",
    sign_in: "تسجيل الدخول",
    caps_lock: "Caps Lock مفعّل.",
    recovery_summary: "الدخول برمز وصول",
    recovery_body: "استخدم رمزًا صادرًا عن مسؤول النظام إذا لم تضبط كلمة المرور بعد.",
    token_label: "رمز الوصول",
    token_hint: "لا يمكن استرجاعه بعد إنشائه.",
    token_submit: "الدخول بالرمز",
    no_mailbox: "ليس لديك صندوق؟",
    create_mailbox_link: "أنشئ صندوقًا",

    heading_signup: "إنشاء صندوق",
    lead_signup: "اختر عنوانًا وكلمة مرور لبدء استخدام بريدك.",
    local_part: "اسم الصندوق",
    local_hint: "حروف لاتينية صغيرة وأرقام، ويمكن استخدام النقطة والشرطة.",
    display_name: "الاسم المعروض",
    optional: "اختياري",
    password_min: "12 حرفًا على الأقل",
    password_hint: "استخدم 12 حرفًا أو أكثر.",
    confirm: "تأكيد كلمة المرور",
    mismatch: "كلمتا المرور غير متطابقتين.",
    create_mailbox: "إنشاء الصندوق",
    have_mailbox: "لديك صندوق؟",
    sign_in_link: "تسجيل الدخول",

    language: "اللغة",
    demo_note: "نسخة تجريبية محلية — بياناتك تبقى على هذا الجهاز.",
    copyright: "© 2026 TIRA LABS",
    skip_to_content: "تجاوز إلى المحتوى",
    loading: "جارٍ التحميل…",
    reveal_show: "إظهار كلمة المرور",
    reveal_hide: "إخفاء كلمة المرور",

    err_bad_email: "أدخل بريدًا إلكترونيًا صحيحًا.",
    err_password_required: "أدخل كلمة المرور.",
    err_password_long: "كلمة المرور طويلة جدًا.",
    err_credentials: "البريد الإلكتروني أو كلمة المرور غير صحيحة.",
    err_token_required: "أدخل رمز الوصول أولًا.",
    err_token_invalid: "الرمز غير صحيح أو منتهي أو ملغى.",
    err_session_expired: "انتهت الجلسة أو أُلغي الرمز. الصق رمزًا جديدًا.",
    err_too_many: "محاولات كثيرة. انتظر عشر دقائق ثم أعد المحاولة.",

    err_local_invalid: "اسم غير صالح: حروف لاتينية صغيرة وأرقام ونقطة أو شرطة، من 2 إلى 32 حرفًا.",
    err_display_long: "الاسم المعروض أطول من 80 حرفًا.",
    err_password_short: "كلمة المرور قصيرة: 12 حرفًا على الأقل.",
    err_password_too_long: "كلمة المرور أطول من 128 حرفًا.",
    err_mismatch: "كلمتا المرور غير متطابقتين.",
    err_taken: "هذا العنوان مستخدم بالفعل. اختر اسمًا آخر.",
};

static FR: AuthText = AuthText {
    title_login: "Connexion",
    title_signup: "Créer une boîte",

    heading_login: "Connexion",
    lead_login: "Saisissez votre adresse e-mail et votre mot de passe pour continuer.",
    email: "Adresse e-mail",
    password: "Mot de passe",
    sign_in: "Se connecter",
    caps_lock: "Verr. Maj. activé.",
    recovery_summary: "Se connecter avec un jeton d'accès",
    recovery_body: "Utilisez un jeton délivré par votre administrateur si vous n'avez pas encore défini de mot de passe.",
    token_label: "Jeton d'accès",
    token_hint: "Il ne peut pas être récupéré après sa création.",
    token_submit: "Se connecter avec le jeton",
    no_mailbox: "Pas encore de boîte ?",
    create_mailbox_link: "Créer une boîte",

    heading_signup: "Créer une boîte",
    lead_signup: "Choisissez une adresse et un mot de passe pour commencer.",
    local_part: "Nom de la boîte",
    local_hint: "Lettres minuscules, chiffres, point et tiret.",
    display_name: "Nom affiché",
    optional: "facultatif",
    password_min: "12 caractères minimum",
    password_hint: "Utilisez au moins 12 caractères.",
    confirm: "Confirmer le mot de passe",
    mismatch: "Les deux mots de passe ne correspondent pas.",
    create_mailbox: "Créer la boîte",
    have_mailbox: "Vous avez déjà une boîte ?",
    sign_in_link: "Se connecter",

    language: "Langue",
    demo_note: "Version de démonstration locale — vos données restent sur cette machine.",
    copyright: "© 2026 TIRA LABS",
    skip_to_content: "Aller au contenu",
    loading: "Chargement…",
    reveal_show: "Afficher le mot de passe",
    reveal_hide: "Masquer le mot de passe",

    err_bad_email: "Saisissez une adresse e-mail valide.",
    err_password_required: "Saisissez votre mot de passe.",
    err_password_long: "Mot de passe trop long.",
    err_credentials: "Adresse e-mail ou mot de passe incorrect.",
    err_token_required: "Saisissez d'abord le jeton d'accès.",
    err_token_invalid: "Jeton invalide, expiré ou révoqué.",
    err_session_expired: "Session expirée ou jeton révoqué. Collez un nouveau jeton.",
    err_too_many: "Trop de tentatives. Patientez dix minutes puis réessayez.",

    err_local_invalid: "Nom invalide : lettres minuscules, chiffres, point ou tiret, de 2 à 32 caractères.",
    err_display_long: "Le nom affiché dépasse 80 caractères.",
    err_password_short: "Mot de passe trop court : 12 caractères minimum.",
    err_password_too_long: "Mot de passe trop long : 128 caractères maximum.",
    err_mismatch: "Les deux mots de passe ne correspondent pas.",
    err_taken: "Cette adresse est déjà utilisée. Choisissez-en une autre.",
};

static EN: AuthText = AuthText {
    title_login: "Sign in",
    title_signup: "Create a mailbox",

    heading_login: "Sign in",
    lead_login: "Enter your email address and password to continue.",
    email: "Email address",
    password: "Password",
    sign_in: "Sign in",
    caps_lock: "Caps Lock is on.",
    recovery_summary: "Sign in with an access token",
    recovery_body: "Use a token issued by your administrator if you have not set a password yet.",
    token_label: "Access token",
    token_hint: "It cannot be retrieved after it is created.",
    token_submit: "Sign in with token",
    no_mailbox: "No mailbox yet?",
    create_mailbox_link: "Create a mailbox",

    heading_signup: "Create a mailbox",
    lead_signup: "Choose an address and a password to get started.",
    local_part: "Mailbox name",
    local_hint: "Lowercase letters, digits, dot and hyphen.",
    display_name: "Display name",
    optional: "optional",
    password_min: "At least 12 characters",
    password_hint: "Use at least 12 characters.",
    confirm: "Confirm password",
    mismatch: "The passwords do not match.",
    create_mailbox: "Create mailbox",
    have_mailbox: "Already have a mailbox?",
    sign_in_link: "Sign in",

    language: "Language",
    demo_note: "Local demo build — your data stays on this machine.",
    copyright: "© 2026 TIRA LABS",
    skip_to_content: "Skip to content",
    loading: "Loading…",
    reveal_show: "Show password",
    reveal_hide: "Hide password",

    err_bad_email: "Enter a valid email address.",
    err_password_required: "Enter your password.",
    err_password_long: "Password is too long.",
    err_credentials: "Incorrect email address or password.",
    err_token_required: "Enter the access token first.",
    err_token_invalid: "The token is invalid, expired or revoked.",
    err_session_expired: "Session expired or token revoked. Paste a new token.",
    err_too_many: "Too many attempts. Wait ten minutes and try again.",

    err_local_invalid:
        "Invalid name: lowercase letters, digits, dot or hyphen, 2 to 32 characters.",
    err_display_long: "Display name is longer than 80 characters.",
    err_password_short: "Password too short: at least 12 characters.",
    err_password_too_long: "Password too long: at most 128 characters.",
    err_mismatch: "The passwords do not match.",
    err_taken: "This address is already taken. Choose another one.",
};

/// Every sentence the mailbox shell and its failure pages render.
///
/// Field names describe the purpose, never the Arabic wording. Values that
/// depend on a number, a status token, or a URL are methods on [`Locale`], not
/// fields, so a translator never has to edit a template.
#[derive(Debug)]
pub struct ShellText {
    // Document chrome shared with the auth screens.
    pub skip_to_content: &'static str,
    pub loading: &'static str,

    // Document titles.
    pub title_inbox: &'static str,
    pub title_compose: &'static str,
    pub title_profile: &'static str,
    pub title_unavailable: &'static str,
    pub title_cross_origin: &'static str,
    pub title_scope: &'static str,

    // Top bar.
    pub language: &'static str,
    pub logout: &'static str,
    pub logout_title: &'static str,

    // Sidebar.
    pub nav_label: &'static str,
    pub compose_new: &'static str,
    pub compose_new_title: &'static str,
    pub folders_label: &'static str,
    pub inbox: &'static str,
    pub inbox_title: &'static str,
    pub profile: &'static str,
    pub profile_title: &'static str,
    pub soon_heading: &'static str,
    pub soon_search: &'static str,
    pub soon_search_why: &'static str,
    pub soon_starred: &'static str,
    pub soon_starred_why: &'static str,
    pub soon_sent: &'static str,
    pub soon_sent_why: &'static str,
    pub soon_archive: &'static str,
    pub soon_archive_why: &'static str,
    pub soon_trash: &'static str,
    pub soon_trash_why: &'static str,
    pub storage: &'static str,
    pub foot_no_sync: &'static str,
    pub foot_session_revocable: &'static str,

    // Split shell.
    pub reader_label: &'static str,

    // Inbox list.
    pub last_updated: &'static str,
    pub refresh: &'static str,
    pub reload_title: &'static str,
    pub no_subject: &'static str,
    pub sent_heading: &'static str,
    pub sent_at: &'static str,
    pub sent_note: &'static str,
    pub empty_heading: &'static str,
    pub empty_body: &'static str,
    pub send_to_self: &'static str,
    pub no_fixtures: &'static str,
    pub choose_message: &'static str,
    pub choose_message_body: &'static str,

    // Reader.
    pub message_tools: &'static str,
    pub message_label: &'static str,
    pub message_details: &'static str,
    pub back_to_inbox: &'static str,
    pub back_to_inbox_title: &'static str,
    pub to: &'static str,
    pub from: &'static str,
    pub subject: &'static str,
    pub body: &'static str,
    pub date: &'static str,
    pub size: &'static str,
    pub received_at: &'static str,
    pub identifier: &'static str,
    pub no_message_id: &'static str,
    pub body_unavailable: &'static str,
    pub html_disabled: &'static str,
    pub attachments_unavailable: &'static str,
    pub attachments: &'static str,
    pub attachments_why: &'static str,
    pub message_no_fixtures: &'static str,
    pub sender_domain_records: &'static str,
    pub auth_scope: &'static str,
    pub auth_missing: &'static str,
    pub print: &'static str,
    pub print_title: &'static str,
    pub download_eml: &'static str,
    pub download_eml_title: &'static str,
    pub reply: &'static str,
    pub reply_title: &'static str,
    pub back_to_message: &'static str,
    pub back_to_message_title: &'static str,
    pub print_hint_before: &'static str,
    pub print_hint_or: &'static str,
    pub print_hint_after: &'static str,

    // Compose.
    pub compose_heading: &'static str,
    pub sender_label: &'static str,
    pub send: &'static str,
    pub cancel: &'static str,
    pub compose_note: &'static str,

    // Compose refusals.
    pub err_recipient_required: &'static str,
    pub err_too_many_recipients: &'static str,
    pub err_body_required: &'static str,
    pub err_body_too_long: &'static str,
    pub err_invalid_message: &'static str,
    pub err_throttled: &'static str,
    pub err_upstream: &'static str,
    pub err_relay_missing: &'static str,
    pub err_generic: &'static str,

    // Profile.
    pub account_security: &'static str,
    pub email: &'static str,
    pub created: &'static str,
    pub messages: &'static str,
    pub mail_app_access: &'static str,
    pub tira_password: &'static str,
    pub password_set: &'static str,
    pub password_not_set: &'static str,
    pub display_name: &'static str,
    pub display_name_help: &'static str,
    pub name: &'static str,
    pub name_placeholder: &'static str,
    pub name_hint: &'static str,
    pub save_name: &'static str,
    pub change_password: &'static str,
    pub set_password: &'static str,
    pub password_help: &'static str,
    pub no_password_notice: &'static str,
    pub current_password: &'static str,
    pub new_password: &'static str,
    pub confirm_password: &'static str,
    pub password_rules: &'static str,
    pub active_sessions: &'static str,
    pub sessions_help: &'static str,
    pub no_sessions: &'static str,
    pub this_session: &'static str,
    pub session_created: &'static str,
    pub last_used: &'static str,
    pub expires: &'static str,
    pub revoke: &'static str,
    pub revoke_others: &'static str,
    pub where_password_works: &'static str,
    pub where_password_body: &'static str,
    pub session_trust_note: &'static str,
    pub never_used: &'static str,
    pub never_expires: &'static str,

    // Profile notices.
    pub status_name_saved: &'static str,
    pub status_password_set: &'static str,
    pub status_password_changed: &'static str,
    pub status_session_revoked: &'static str,
    pub status_others_revoked: &'static str,
    pub status_no_other_sessions: &'static str,
    pub err_name_too_long: &'static str,
    pub err_password_short: &'static str,
    pub err_password_long: &'static str,
    pub err_password_mismatch: &'static str,
    pub err_password_same: &'static str,
    pub err_password_equals_email: &'static str,
    pub err_current_required: &'static str,
    pub err_current_wrong: &'static str,
    pub err_rate_limited: &'static str,
    pub err_session_not_found: &'static str,
    pub err_hash_failed: &'static str,

    // Failure pages.
    pub problem_bad_link_eyebrow: &'static str,
    pub problem_bad_link_title: &'static str,
    pub problem_bad_link_message: &'static str,
    pub problem_bad_message_link_message: &'static str,
    pub problem_bad_request_eyebrow: &'static str,
    pub problem_bad_request_title: &'static str,
    pub problem_bad_request_message: &'static str,
    pub problem_not_found_eyebrow: &'static str,
    pub problem_missing_mailbox: &'static str,
    pub problem_missing_message: &'static str,
    pub problem_wrong_mailbox: &'static str,
    pub problem_wrong_mailbox_message: &'static str,
    pub problem_check_link: &'static str,
    pub problem_deleted: &'static str,
    pub problem_notice: &'static str,
    pub back_to_login: &'static str,

    pub unavailable_heading: &'static str,
    pub unavailable_body: &'static str,
    pub unavailable_help: &'static str,
    pub retry: &'static str,

    pub scope_eyebrow: &'static str,
    pub scope_title: &'static str,
    pub scope_body: &'static str,
    pub scope_notice: &'static str,

    pub csrf_eyebrow: &'static str,
    pub csrf_title: &'static str,
    pub csrf_body: &'static str,
    pub csrf_notice: &'static str,
    pub csrf_back: &'static str,

    pub local_domain_eyebrow: &'static str,
    pub local_domain_title: &'static str,
}

static SHELL_AR: ShellText = ShellText {
    skip_to_content: "انتقل إلى المحتوى",
    loading: "جارٍ التحميل…",
    title_inbox: "الوارد",
    title_compose: "تحرير الرسالة",
    title_profile: "الملف الشخصي",
    title_unavailable: "تعذّر التحميل",
    title_cross_origin: "أصل الطلب غير موثوق",
    title_scope: "اختر صندوقًا",

    language: "اللغة",
    logout: "تسجيل الخروج",
    logout_title: "حذف كوكي الجلسة من هذا المتصفح",

    nav_label: "التنقّل",
    compose_new: "رسالة جديدة",
    compose_new_title: "تحرير رسالة جديدة من هذا الصندوق",
    folders_label: "المجلدات",
    inbox: "الوارد",
    inbox_title: "الوارد — الرسائل المخزّنة في هذا الصندوق",
    profile: "الملف الشخصي",
    profile_title: "الملف الشخصي — الاسم وكلمة المرور والجلسات",
    soon_heading: "غير متاح بعد",
    soon_search: "بحث في البريد",
    soon_search_why: "لا فهرس بحث بعد",
    soon_starred: "المميّز",
    soon_starred_why: "لا تصنيف بعد",
    soon_sent: "المُرسَل",
    soon_sent_why: "لا عرض لمجلد المُرسَل بعد",
    soon_archive: "المؤرشف",
    soon_archive_why: "لا مجلدات بعد",
    soon_trash: "المهملات",
    soon_trash_why: "الحذف غير متاح بعد",
    storage: "المساحة",
    foot_no_sync: "لا مزامنة مع خدمات خارجية",
    foot_session_revocable: "الجلسة قابلة للإلغاء من الملف الشخصي",

    reader_label: "قارئ الرسالة",

    last_updated: "آخر تحديث",
    refresh: "تحديث",
    reload_title: "إعادة تحميل القائمة",
    no_subject: "(بلا موضوع)",
    sent_heading: "تم إرسال الرسالة",
    sent_at: "وقت الإرسال",
    sent_note: "استلم خادم البريد الرسالة. هذا لا يثبت وصولها إلى صندوق المستلم، ولا تُحفظ نسخة في مجلد «مُرسَل» بعد.",
    empty_heading: "لا توجد رسائل بعد",
    empty_body: "لا رسائل نموذجية هنا: أول رسالة تقرأها هي التي ترسلها إلى نفسك.",
    send_to_self: "أرسل رسالة إلى نفسك",
    no_fixtures: "لا توجد بيانات تجريبية في هذا الصندوق.",
    choose_message: "اختر رسالة",
    choose_message_body: "اختر رسالة من القائمة لعرض نصّها في هذه المساحة.",

    message_tools: "أدوات الرسالة",
    message_label: "الرسالة",
    message_details: "بيانات الرسالة",
    back_to_inbox: "الوارد",
    back_to_inbox_title: "العودة إلى قائمة الوارد",
    to: "إلى",
    from: "من",
    subject: "الموضوع",
    body: "النص",
    date: "التاريخ",
    size: "الحجم",
    received_at: "تاريخ الاستلام",
    identifier: "المعرّف",
    no_message_id: "غير موجود في الترويسة",
    body_unavailable: "تعذّر استخراج جسم نصّي من هذه الرسالة (قد تكون HTML فقط أو غير مكتملة).",
    html_disabled: "عرض HTML والصور والمرفقات معطّل: التصيير الآمن غير منفّذ بعد، فيُعرض النص فقط ولا تُحمَّل أي صورة خارجية.",
    attachments_unavailable: "المرفقات — غير متاح بعد",
    attachments: "المرفقات",
    attachments_why: "دعم المرفقات غير مربوط بعد",
    message_no_fixtures: "هذه الرسالة مخصصة لهذا الصندوق — لا بيانات تجريبية.",
    sender_domain_records: "سجلات نطاق المرسل",
    auth_scope: "فحوص سجلات DNS المنشورة لهذا النطاق، لا نتيجة المصادقة لهذه الرسالة: لا يُفحص توقيع DKIM ولا نتيجة SPF/DMARC الفعلية عند الاستلام.",
    auth_missing: "تعذّر قراءة سجلات DNS المخزّنة لهذا النطاق؛ لا يُعرض أي حكم بدلًا من ذلك.",
    print: "نسخة للطباعة",
    print_title: "نسخة مهيّأة للورق — تُفتح في صفحة مستقلة",
    download_eml: "تنزيل .eml",
    download_eml_title: "تنزيل الرسالة الأصلية بصيغة .eml",
    reply: "الرد",
    reply_title: "الرد إلى المرسل في صندوقك",
    back_to_message: "رجوع إلى الرسالة",
    back_to_message_title: "العودة إلى عرض الرسالة",
    print_hint_before: "مهيّأة للورق: استخدم ",
    print_hint_or: " أو ",
    print_hint_after: ". لا صور ولا HTML.",

    compose_heading: "تحرير الرسالة",
    sender_label: "المرسل: ",
    send: "إرسال",
    cancel: "إلغاء",
    compose_note: "تُرسل الرسالة من صندوقك الحالي إلى العنوان الذي تكتبه في حقل «إلى».",

    err_recipient_required: "البريد الإلكتروني مطلوب.",
    err_too_many_recipients: "عدد المستلمين أكبر من المسموح.",
    err_body_required: "جسم الرسالة مطلوب.",
    err_body_too_long: "الرسالة أطول من المسموح.",
    err_invalid_message: "البريد الإلكتروني أو الرسالة غير صالحة.",
    err_throttled: "محاولات كثيرة. انتظر قبل إعادة المحاولة.",
    err_upstream: "تعذّر إرسال الرسالة. حاول مرة أخرى.",
    err_relay_missing: "الإرسال غير مهيّأ على هذه النسخة بعد.",
    err_generic: "تعذّر معالجة الطلب. حاول مرة أخرى.",

    account_security: "الحساب والأمان",
    email: "البريد",
    created: "أُنشئ",
    messages: "الرسائل",
    mail_app_access: "الوصول من برامج البريد",
    tira_password: "كلمة مرور Tira",
    password_set: "مضبوطة",
    password_not_set: "غير مضبوطة",
    display_name: "الاسم الظاهر",
    display_name_help: "يظهر في الشريط العلوي وفي قائمة الجلسات. لا يغيّر عنوان البريد ولا يُستخدم في الدخول.",
    name: "الاسم",
    name_placeholder: "مثال: ياسين",
    name_hint: "اتركه فارغًا للعودة إلى اسم العنوان المحلي.",
    save_name: "حفظ الاسم",
    change_password: "تغيير كلمة المرور",
    set_password: "تعيين كلمة المرور",
    password_help: "كلمة المرور التي تستخدمها للدخول إلى Tira. تُحفظ مشفّرة ولا يمكن قراءتها، ولا تُرسل إلى أي جهة.",
    no_password_notice: "لا توجد كلمة مرور بعد. عيّن واحدة الآن، وستبقى الجلسة الحالية صالحة.",
    current_password: "كلمة المرور الحالية",
    new_password: "كلمة المرور الجديدة",
    confirm_password: "تأكيد كلمة المرور",
    password_rules: "12 حرفًا على الأقل، و128 كحد أقصى. لا تستخدم بريدك.",
    active_sessions: "الجلسات النشطة",
    sessions_help: "كل صف جلسة مرتبط بهذا الصندوق. الإلغاء فوري ولا يمكن التراجع عنه.",
    no_sessions: "لا توجد جلسات نشطة.",
    this_session: "هذه الجلسة",
    session_created: "أُنشئت",
    last_used: "آخر استخدام",
    expires: "تنتهي",
    revoke: "إلغاء",
    revoke_others: "إلغاء كل الجلسات الأخرى",
    where_password_works: "أين تعمل هذه الكلمة",
    where_password_body: "هذه الكلمة تفتح Tira على الويب فقط. إن كنت تقرأ بريدك من تطبيق على جهازك، فله بيانات اعتماد خاصة به، وهذا النموذج لا يغيّرها.",
    session_trust_note: "ثقة الجلسة تبقى: هذا النموذج لا يخزّن الأسرار.",
    never_used: "لم تُستخدم بعد",
    never_expires: "لا تنتهي",

    status_name_saved: "حُفظ الاسم الظاهر.",
    status_password_set: "ضُبطت كلمة المرور. يمكنك الآن الدخول بها بدل الرمز.",
    status_password_changed: "تغيّرت كلمة المرور، وأُلغيت كل الجلسات الأخرى على هذا الصندوق.",
    status_session_revoked: "أُلغيت الجلسة المحددة.",
    status_others_revoked: "أُلغيت كل الجلسات الأخرى.",
    status_no_other_sessions: "لا توجد جلسات أخرى نشطة.",
    err_name_too_long: "الاسم الظاهر طويل جدًا: الحد 80 حرفًا.",
    err_password_short: "كلمة المرور قصيرة: 12 حرفًا على الأقل.",
    err_password_long: "كلمة المرور طويلة جدًا: الحد 128 حرفًا.",
    err_password_mismatch: "تأكيد كلمة المرور لا يطابقها.",
    err_password_same: "اختر كلمة مرور مختلفة عن الحالية.",
    err_password_equals_email: "لا تستخدم بريدك أو اسمه المحلي ككلمة مرور.",
    err_current_required: "أدخل كلمة المرور الحالية أولًا.",
    err_current_wrong: "كلمة المرور الحالية غير صحيحة.",
    err_rate_limited: "محاولات كثيرة. انتظر عشر دقائق ثم أعد المحاولة.",
    err_session_not_found: "الجلسة غير موجودة أو أُلغيت مسبقًا.",
    err_hash_failed: "تعذّر تجهيز كلمة المرور. أعد المحاولة.",

    problem_bad_link_eyebrow: "رابط غير صالح",
    problem_bad_link_title: "الرابط غير مكتمل.",
    problem_bad_link_message: "افتح صندوقك من صفحة الدخول ثم أعد المحاولة.",
    problem_bad_message_link_message: "افتح الصندوق من صفحته ثم اختر الرسالة من القائمة.",
    problem_bad_request_eyebrow: "بيانات غير صالحة",
    problem_bad_request_title: "هذا الطلب لا يحتوي على معرّف صندوق صالح.",
    problem_bad_request_message: "المعرّف المطلوب ناقص أو غير صالح، لذلك لا يمكن الإنشاء.",
    problem_not_found_eyebrow: "غير موجود",
    problem_missing_mailbox: "لا يوجد صندوق بهذا المعرّف.",
    problem_missing_message: "لا توجد رسالة بهذا المعرّف.",
    problem_wrong_mailbox: "لا توجد رسالة بهذا المعرّف في هذا الصندوق.",
    problem_wrong_mailbox_message: "الرسالة قد تكون في صندوق آخر.",
    problem_check_link: "تحقّق من الرابط أو ابدأ من صندوقك.",
    problem_deleted: "قد يكون الصندوق قد حُذف.",
    problem_notice: "لم يُعرض أي محتوى. تحقّق من الرابط، أو ابدأ من صندوقك.",
    back_to_login: "العودة إلى تسجيل الدخول",

    unavailable_heading: "تعذّر تحميل بريدك الآن.",
    unavailable_body: "حدث عطل مؤقت في الخدمة. لم يتغيّر شيء في حسابك؛ أعد المحاولة بعد لحظات.",
    unavailable_help: "إن تكرّر العطل، أبلغ مسؤول هذه النسخة.",
    retry: "إعادة المحاولة",

    scope_eyebrow: "نوع الوصول",
    scope_title: "هذا الوصول لا يحدّد صندوقًا واحدًا.",
    scope_body: "الوصول الحالي يغطّي كل الصناديق في حسابك، وواجهة القراءة تحتاج صندوقًا واحدًا لتفتحه.",
    scope_notice: "افتح الرابط الخاص بصندوق محدّد، أو سجّل الدخول بكلمة المرور لاختيار صندوقك.",

    csrf_eyebrow: "حماية CSRF",
    csrf_title: "أصل الطلب غير موثوق.",
    csrf_body: "رُفض هذا الطلب لأن المتصفح أرسله من أصل مختلف أو غير قابل للتحقق. هذه حماية من تزوير الطلبات عبر المواقع، ولم تُنفَّذ أي عملية.",
    csrf_notice: "لم يتغيّر أي شيء. أعد المحاولة من صفحة الدخول داخل هذا الموقع، أو حدّث الصفحة إن كنت تستخدم متصفحًا مضمّنًا.",
    csrf_back: "العودة إلى تسجيل الدخول",

    local_domain_eyebrow: "التهيئة المحلية",
    local_domain_title: "النطاق المحلي غير موجود",
};

static SHELL_FR: ShellText = ShellText {
    skip_to_content: "Aller au contenu",
    loading: "Chargement…",
    title_inbox: "Boîte de réception",
    title_compose: "Rédiger un message",
    title_profile: "Profil",
    title_unavailable: "Chargement impossible",
    title_cross_origin: "Origine de la requête non fiable",
    title_scope: "Choisir une boîte",

    language: "Langue",
    logout: "Se déconnecter",
    logout_title: "Supprimer le cookie de session de ce navigateur",

    nav_label: "Navigation",
    compose_new: "Nouveau message",
    compose_new_title: "Rédiger un nouveau message depuis cette boîte",
    folders_label: "Dossiers",
    inbox: "Boîte de réception",
    inbox_title: "Boîte de réception — messages stockés dans cette boîte",
    profile: "Profil",
    profile_title: "Profil — nom, mot de passe et sessions",
    soon_heading: "Pas encore disponible",
    soon_search: "Recherche dans les messages",
    soon_search_why: "Pas encore d'index de recherche",
    soon_starred: "Favoris",
    soon_starred_why: "Pas encore de classement",
    soon_sent: "Envoyés",
    soon_sent_why: "Pas encore de vue des messages envoyés",
    soon_archive: "Archives",
    soon_archive_why: "Pas encore de dossiers",
    soon_trash: "Corbeille",
    soon_trash_why: "Suppression pas encore disponible",
    storage: "Espace",
    foot_no_sync: "Aucune synchronisation avec des services externes",
    foot_session_revocable: "La session peut être révoquée depuis le profil",

    reader_label: "Lecteur de message",

    last_updated: "dernière mise à jour",
    refresh: "Actualiser",
    reload_title: "Recharger la liste",
    no_subject: "(sans objet)",
    sent_heading: "Message envoyé",
    sent_at: "Heure d'envoi",
    sent_note: "Le serveur de messagerie a accepté le message. Cela ne prouve pas sa remise dans la boîte du destinataire, et aucune copie n'est conservée dans un dossier « Envoyés » pour l'instant.",
    empty_heading: "Aucun message pour l'instant",
    empty_body: "Aucun message d'exemple ici : le premier message que vous lisez est celui que vous vous envoyez.",
    send_to_self: "M'envoyer un message",
    no_fixtures: "Aucune donnée de démonstration dans cette boîte.",
    choose_message: "Choisir un message",
    choose_message_body: "Choisissez un message dans la liste pour afficher son texte ici.",

    message_tools: "Outils du message",
    message_label: "Message",
    message_details: "Détails du message",
    back_to_inbox: "Boîte de réception",
    back_to_inbox_title: "Retour à la liste de la boîte de réception",
    to: "À",
    from: "De",
    subject: "Objet",
    body: "Texte",
    date: "Date",
    size: "Taille",
    received_at: "Date de réception",
    identifier: "Identifiant",
    no_message_id: "Absent de l'en-tête",
    body_unavailable: "Impossible d'extraire un corps texte de ce message (il peut être en HTML seul ou incomplet).",
    html_disabled: "L'affichage HTML, des images et des pièces jointes est désactivé : le rendu sécurisé n'est pas encore en place, seul le texte est affiché et aucune image externe n'est chargée.",
    attachments_unavailable: "Pièces jointes — pas encore disponibles",
    attachments: "Pièces jointes",
    attachments_why: "La prise en charge des pièces jointes n'est pas encore branchée",
    message_no_fixtures: "Ce message appartient à cette boîte — aucune donnée de démonstration.",
    sender_domain_records: "Enregistrements du domaine de l'expéditeur",
    auth_scope: "Enregistrements DNS publiés pour ce domaine, et non le résultat d'authentification de ce message : la signature DKIM et le résultat SPF/DMARC réel à la réception ne sont pas vérifiés.",
    auth_missing: "Impossible de lire les enregistrements DNS stockés pour ce domaine ; aucun verdict n'est affiché à la place.",
    print: "Version imprimable",
    print_title: "Version papier — s'ouvre dans une page séparée",
    download_eml: "Télécharger .eml",
    download_eml_title: "Télécharger le message d'origine au format .eml",
    reply: "Répondre",
    reply_title: "Répondre à l'expéditeur depuis votre boîte",
    back_to_message: "Retour au message",
    back_to_message_title: "Retour à l'affichage du message",
    print_hint_before: "Version papier : utilisez ",
    print_hint_or: " ou ",
    print_hint_after: ". Ni images ni HTML.",

    compose_heading: "Rédiger un message",
    sender_label: "Expéditeur : ",
    send: "Envoyer",
    cancel: "Annuler",
    compose_note: "Le message part de votre boîte actuelle vers l'adresse saisie dans le champ « À ».",

    err_recipient_required: "L'adresse du destinataire est obligatoire.",
    err_too_many_recipients: "Trop de destinataires.",
    err_body_required: "Le texte du message est obligatoire.",
    err_body_too_long: "Le message dépasse la taille autorisée.",
    err_invalid_message: "L'adresse ou le message n'est pas valide.",
    err_throttled: "Trop d'envois. Patientez avant de réessayer.",
    err_upstream: "L'envoi a échoué. Réessayez.",
    err_relay_missing: "L'envoi n'est pas encore configuré sur cette version.",
    err_generic: "Impossible de traiter la demande. Réessayez.",

    account_security: "Compte et sécurité",
    email: "Adresse e-mail",
    created: "Créée",
    messages: "Messages",
    mail_app_access: "Accès depuis un logiciel de messagerie",
    tira_password: "Mot de passe Tira",
    password_set: "défini",
    password_not_set: "non défini",
    display_name: "Nom affiché",
    display_name_help: "Il apparaît dans la barre supérieure et la liste des sessions. Il ne change pas l'adresse et ne sert pas à la connexion.",
    name: "Nom",
    name_placeholder: "Exemple : Yassine",
    name_hint: "Laissez vide pour revenir au nom de l'adresse locale.",
    save_name: "Enregistrer le nom",
    change_password: "Changer le mot de passe",
    set_password: "Définir un mot de passe",
    password_help: "Le mot de passe qui vous connecte à Tira. Il est stocké de façon chiffrée, ne peut pas être lu et n'est envoyé à personne.",
    no_password_notice: "Aucun mot de passe pour l'instant. Définissez-en un maintenant ; la session actuelle reste valide.",
    current_password: "Mot de passe actuel",
    new_password: "Nouveau mot de passe",
    confirm_password: "Confirmer le mot de passe",
    password_rules: "12 caractères minimum, 128 maximum. N'utilisez pas votre adresse.",
    active_sessions: "Sessions actives",
    sessions_help: "Chaque session est liée à cette boîte. La révocation est immédiate et définitive.",
    no_sessions: "Aucune session active.",
    this_session: "Cette session",
    session_created: "Créée",
    last_used: "Dernière utilisation",
    expires: "Expire",
    revoke: "Révoquer",
    revoke_others: "Révoquer toutes les autres sessions",
    where_password_works: "Où ce mot de passe agit",
    where_password_body: "Ce mot de passe ouvre Tira sur le web uniquement. Si vous lisez votre courrier depuis une application sur votre appareil, elle a ses propres identifiants, que ce formulaire ne modifie pas.",
    session_trust_note: "La confiance de session demeure : ce formulaire ne stocke aucun secret.",
    never_used: "Jamais utilisée",
    never_expires: "N'expire pas",

    status_name_saved: "Nom affiché enregistré.",
    status_password_set: "Mot de passe défini. Vous pouvez désormais vous connecter avec lui au lieu du jeton.",
    status_password_changed: "Mot de passe modifié ; toutes les autres sessions de cette boîte ont été révoquées.",
    status_session_revoked: "Session révoquée.",
    status_others_revoked: "Toutes les autres sessions ont été révoquées.",
    status_no_other_sessions: "Aucune autre session active.",
    err_name_too_long: "Nom affiché trop long : 80 caractères maximum.",
    err_password_short: "Mot de passe trop court : 12 caractères minimum.",
    err_password_long: "Mot de passe trop long : 128 caractères maximum.",
    err_password_mismatch: "La confirmation ne correspond pas au mot de passe.",
    err_password_same: "Choisissez un mot de passe différent de l'actuel.",
    err_password_equals_email: "N'utilisez pas votre adresse ni son nom local comme mot de passe.",
    err_current_required: "Saisissez d'abord le mot de passe actuel.",
    err_current_wrong: "Mot de passe actuel incorrect.",
    err_rate_limited: "Trop de tentatives. Patientez dix minutes puis réessayez.",
    err_session_not_found: "Session introuvable ou déjà révoquée.",
    err_hash_failed: "Impossible de préparer le mot de passe. Réessayez.",

    problem_bad_link_eyebrow: "Lien invalide",
    problem_bad_link_title: "Le lien est incomplet.",
    problem_bad_link_message: "Ouvrez votre boîte depuis la page de connexion puis réessayez.",
    problem_bad_message_link_message: "Ouvrez la boîte depuis sa page puis choisissez le message dans la liste.",
    problem_bad_request_eyebrow: "Données invalides",
    problem_bad_request_title: "Cette requête ne contient pas d'identifiant de boîte valide.",
    problem_bad_request_message: "L'identifiant requis est absent ou invalide ; la création est impossible.",
    problem_not_found_eyebrow: "Introuvable",
    problem_missing_mailbox: "Aucune boîte ne correspond à cet identifiant.",
    problem_missing_message: "Aucun message ne correspond à cet identifiant.",
    problem_wrong_mailbox: "Aucun message de cet identifiant dans cette boîte.",
    problem_wrong_mailbox_message: "Le message appartient peut-être à une autre boîte.",
    problem_check_link: "Vérifiez le lien ou partez de votre boîte.",
    problem_deleted: "La boîte a peut-être été supprimée.",
    problem_notice: "Aucun contenu n'a été affiché. Vérifiez le lien, ou partez de votre boîte.",
    back_to_login: "Retour à la connexion",

    unavailable_heading: "Impossible de charger votre courrier pour l'instant.",
    unavailable_body: "Un incident temporaire a touché le service. Rien n'a changé dans votre compte ; réessayez dans un instant.",
    unavailable_help: "Si l'incident persiste, signalez-le à la personne qui gère cette instance.",
    retry: "Réessayer",

    scope_eyebrow: "Type d'accès",
    scope_title: "Cet accès ne désigne pas une seule boîte.",
    scope_body: "L'accès actuel couvre toutes les boîtes de votre compte, et l'interface de lecture a besoin d'une boîte précise à ouvrir.",
    scope_notice: "Ouvrez le lien propre à une boîte, ou connectez-vous avec votre mot de passe pour choisir votre boîte.",

    csrf_eyebrow: "Protection CSRF",
    csrf_title: "Origine de la requête non fiable.",
    csrf_body: "Cette requête a été refusée car le navigateur l'a envoyée depuis une origine différente ou non vérifiable. C'est une protection contre la falsification de requêtes intersites ; aucune opération n'a été exécutée.",
    csrf_notice: "Rien n'a changé. Réessayez depuis la page de connexion de ce site, ou actualisez la page si vous utilisez un navigateur intégré.",
    csrf_back: "Retour à la connexion",

    local_domain_eyebrow: "Configuration locale",
    local_domain_title: "Le domaine local est absent",
};

static SHELL_EN: ShellText = ShellText {
    skip_to_content: "Skip to content",
    loading: "Loading…",
    title_inbox: "Inbox",
    title_compose: "Compose message",
    title_profile: "Profile",
    title_unavailable: "Could not load",
    title_cross_origin: "Untrusted request origin",
    title_scope: "Choose a mailbox",

    language: "Language",
    logout: "Sign out",
    logout_title: "Delete the session cookie from this browser",

    nav_label: "Navigation",
    compose_new: "New message",
    compose_new_title: "Compose a new message from this mailbox",
    folders_label: "Folders",
    inbox: "Inbox",
    inbox_title: "Inbox — messages stored in this mailbox",
    profile: "Profile",
    profile_title: "Profile — name, password and sessions",
    soon_heading: "Not available yet",
    soon_search: "Search mail",
    soon_search_why: "No search index yet",
    soon_starred: "Starred",
    soon_starred_why: "No labelling yet",
    soon_sent: "Sent",
    soon_sent_why: "No sent view yet",
    soon_archive: "Archive",
    soon_archive_why: "No folders yet",
    soon_trash: "Trash",
    soon_trash_why: "Deletion not available yet",
    storage: "Storage",
    foot_no_sync: "No synchronisation with external services",
    foot_session_revocable: "The session can be revoked from the profile",

    reader_label: "Message reader",

    last_updated: "last updated",
    refresh: "Refresh",
    reload_title: "Reload the list",
    no_subject: "(no subject)",
    sent_heading: "Message sent",
    sent_at: "Sent at",
    sent_note: "The mail server accepted the message. This does not prove delivery to the recipient's mailbox, and no copy is kept in a Sent folder yet.",
    empty_heading: "No messages yet",
    empty_body: "No sample messages here: the first message you read is the one you send to yourself.",
    send_to_self: "Send a message to yourself",
    no_fixtures: "No demo data in this mailbox.",
    choose_message: "Choose a message",
    choose_message_body: "Choose a message from the list to show its text here.",

    message_tools: "Message tools",
    message_label: "Message",
    message_details: "Message details",
    back_to_inbox: "Inbox",
    back_to_inbox_title: "Back to the inbox list",
    to: "To",
    from: "From",
    subject: "Subject",
    body: "Body",
    date: "Date",
    size: "Size",
    received_at: "Received",
    identifier: "Identifier",
    no_message_id: "Not present in the header",
    body_unavailable: "Could not extract a text body from this message (it may be HTML-only or incomplete).",
    html_disabled: "HTML, images and attachments are disabled: safe rendering is not implemented yet, so only text is shown and no external image is loaded.",
    attachments_unavailable: "Attachments — not available yet",
    attachments: "Attachments",
    attachments_why: "Attachment support is not wired yet",
    message_no_fixtures: "This message belongs to this mailbox — no demo data.",
    sender_domain_records: "Sender domain records",
    auth_scope: "Published DNS records for this domain, not this message's authentication result: the DKIM signature and the actual SPF/DMARC result at receipt are not checked.",
    auth_missing: "Could not read the stored DNS records for this domain; no verdict is shown instead.",
    print: "Print view",
    print_title: "Paper-ready view — opens in a separate page",
    download_eml: "Download .eml",
    download_eml_title: "Download the original message as .eml",
    reply: "Reply",
    reply_title: "Reply to the sender from your mailbox",
    back_to_message: "Back to message",
    back_to_message_title: "Back to the message view",
    print_hint_before: "Paper-ready: use ",
    print_hint_or: " or ",
    print_hint_after: ". No images, no HTML.",

    compose_heading: "Compose message",
    sender_label: "Sender: ",
    send: "Send",
    cancel: "Cancel",
    compose_note: "The message goes from your current mailbox to the address you type in the “To” field.",

    err_recipient_required: "A recipient address is required.",
    err_too_many_recipients: "Too many recipients.",
    err_body_required: "The message body is required.",
    err_body_too_long: "The message is longer than allowed.",
    err_invalid_message: "The address or message is not valid.",
    err_throttled: "Too many sends. Please wait before retrying.",
    err_upstream: "The message could not be sent. Try again.",
    err_relay_missing: "Sending is not configured on this build yet.",
    err_generic: "The request could not be processed. Try again.",

    account_security: "Account and security",
    email: "Email",
    created: "Created",
    messages: "Messages",
    mail_app_access: "Access from mail apps",
    tira_password: "Tira password",
    password_set: "set",
    password_not_set: "not set",
    display_name: "Display name",
    display_name_help: "It appears in the top bar and the session list. It does not change the address and is not used to sign in.",
    name: "Name",
    name_placeholder: "e.g. Yassine",
    name_hint: "Leave empty to fall back to the local address name.",
    save_name: "Save name",
    change_password: "Change password",
    set_password: "Set a password",
    password_help: "The password that signs you in to Tira. It is stored encrypted, cannot be read, and is not sent anywhere.",
    no_password_notice: "No password yet. Set one now; the current session stays valid.",
    current_password: "Current password",
    new_password: "New password",
    confirm_password: "Confirm password",
    password_rules: "At least 12 characters, at most 128. Do not use your address.",
    active_sessions: "Active sessions",
    sessions_help: "Each session is tied to this mailbox. Revocation is immediate and cannot be undone.",
    no_sessions: "No active sessions.",
    this_session: "This session",
    session_created: "Created",
    last_used: "Last used",
    expires: "Expires",
    revoke: "Revoke",
    revoke_others: "Revoke all other sessions",
    where_password_works: "Where this password works",
    where_password_body: "This password opens Tira on the web only. If you read your mail from an app on your device, it has its own credentials, and this form does not change them.",
    session_trust_note: "Session trust holds: this form stores no secrets.",
    never_used: "Never used",
    never_expires: "Never expires",

    status_name_saved: "Display name saved.",
    status_password_set: "Password set. You can now sign in with it instead of the token.",
    status_password_changed: "Password changed; every other session on this mailbox was revoked.",
    status_session_revoked: "Session revoked.",
    status_others_revoked: "All other sessions were revoked.",
    status_no_other_sessions: "No other active sessions.",
    err_name_too_long: "Display name too long: 80 characters maximum.",
    err_password_short: "Password too short: at least 12 characters.",
    err_password_long: "Password too long: 128 characters maximum.",
    err_password_mismatch: "The confirmation does not match the password.",
    err_password_same: "Choose a password different from the current one.",
    err_password_equals_email: "Do not use your address or its local name as a password.",
    err_current_required: "Enter the current password first.",
    err_current_wrong: "Current password is incorrect.",
    err_rate_limited: "Too many attempts. Wait ten minutes and try again.",
    err_session_not_found: "Session not found or already revoked.",
    err_hash_failed: "Could not prepare the password. Try again.",

    problem_bad_link_eyebrow: "Invalid link",
    problem_bad_link_title: "The link is incomplete.",
    problem_bad_link_message: "Open your mailbox from the sign-in page and try again.",
    problem_bad_message_link_message: "Open the mailbox from its page, then choose the message from the list.",
    problem_bad_request_eyebrow: "Invalid data",
    problem_bad_request_title: "This request has no valid mailbox identifier.",
    problem_bad_request_message: "The required identifier is missing or invalid, so nothing could be created.",
    problem_not_found_eyebrow: "Not found",
    problem_missing_mailbox: "No mailbox matches this identifier.",
    problem_missing_message: "No message matches this identifier.",
    problem_wrong_mailbox: "No message with this identifier in this mailbox.",
    problem_wrong_mailbox_message: "The message may be in another mailbox.",
    problem_check_link: "Check the link or start from your mailbox.",
    problem_deleted: "The mailbox may have been deleted.",
    problem_notice: "No content was shown. Check the link, or start from your mailbox.",
    back_to_login: "Back to sign in",

    unavailable_heading: "Your mail cannot be loaded right now.",
    unavailable_body: "A temporary service fault occurred. Nothing changed in your account; try again in a moment.",
    unavailable_help: "If the fault repeats, tell the person running this instance.",
    retry: "Try again",

    scope_eyebrow: "Access type",
    scope_title: "This access does not name a single mailbox.",
    scope_body: "The current access covers every mailbox in your account, and the reading interface needs one mailbox to open.",
    scope_notice: "Open the link for a specific mailbox, or sign in with your password to choose your mailbox.",

    csrf_eyebrow: "CSRF protection",
    csrf_title: "Untrusted request origin.",
    csrf_body: "This request was refused because the browser sent it from a different or unverifiable origin. This protects against cross-site request forgery; no operation ran.",
    csrf_notice: "Nothing changed. Try again from the sign-in page inside this site, or reload the page if you are using an embedded browser.",
    csrf_back: "Back to sign in",

    local_domain_eyebrow: "Local setup",
    local_domain_title: "The local domain is missing",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_has_a_direction_and_a_label() {
        assert_eq!(Locale::Ar.dir(), "rtl");
        assert_eq!(Locale::Fr.dir(), "ltr");
        assert_eq!(Locale::En.dir(), "ltr");
        assert_eq!(Locale::Ar.code(), "ar");
        assert_eq!(Locale::Fr.code(), "fr");
        assert_eq!(Locale::En.code(), "en");
        assert_eq!(Locale::ALL.len(), 3);
        for locale in Locale::ALL {
            assert!(!locale.label().is_empty());
            assert!(!locale.auth().sign_in.is_empty());
        }
    }

    #[test]
    fn codes_and_accept_language_are_parsed_without_guessing() {
        assert_eq!(Locale::from_code("fr-MA"), Some(Locale::Fr));
        assert_eq!(Locale::from_code("EN_us"), Some(Locale::En));
        assert_eq!(Locale::from_code("de"), None);
        assert_eq!(
            Locale::from_accept_language("fr-FR,fr;q=0.9,en;q=0.8"),
            Some(Locale::Fr)
        );
        assert_eq!(Locale::from_accept_language("de-DE,de;q=0.9"), None);
        assert_eq!(Locale::from_accept_language(""), None);
    }

    #[test]
    fn translations_are_distinct_and_free_of_internal_jargon() {
        let ar = Locale::Ar.auth();
        let fr = Locale::Fr.auth();
        let en = Locale::En.auth();
        assert_ne!(fr.sign_in, en.sign_in);
        assert_ne!(ar.sign_in, en.sign_in);

        // The public screens never name the database, the hash function, or the
        // mail engine. Those are implementation details, not user vocabulary.
        for locale in Locale::ALL {
            let text = locale.auth();
            for value in [
                text.lead_login,
                text.lead_signup,
                text.recovery_body,
                text.password_hint,
                text.demo_note,
            ] {
                let lowered = value.to_ascii_lowercase();
                assert!(!lowered.contains("postgres"), "{value}");
                assert!(!lowered.contains("argon2"), "{value}");
                assert!(!lowered.contains("rls"), "{value}");
                assert!(!lowered.contains("stalwart"), "{value}");
                assert!(!lowered.contains("imap"), "{value}");
                assert!(!lowered.contains("sql"), "{value}");
            }
        }
    }

    #[test]
    fn every_locale_has_a_full_shell() {
        for locale in Locale::ALL {
            let text = locale.shell();
            assert!(!text.inbox.is_empty());
            assert!(!text.compose_new.is_empty());
            assert!(!text.profile.is_empty());
            assert!(!text.send.is_empty());
            assert!(!text.retry.is_empty());
            assert!(!text.csrf_title.is_empty());
            assert!(!locale.meta_description().is_empty());
        }
        assert_ne!(Locale::Ar.shell().send, Locale::En.shell().send);
        assert_ne!(Locale::Fr.shell().send, Locale::En.shell().send);
    }

    #[test]
    fn arabic_counts_use_all_six_plural_forms() {
        assert_eq!(Locale::Ar.messages_count(0), "لا رسائل");
        assert_eq!(Locale::Ar.messages_count(1), "رسالة واحدة");
        assert_eq!(Locale::Ar.messages_count(2), "رسالتان");
        assert_eq!(Locale::Ar.messages_count(3), "3 رسائل");
        assert_eq!(Locale::Ar.messages_count(10), "10 رسائل");
        assert_eq!(Locale::Ar.messages_count(11), "11 رسالة");
        assert_eq!(Locale::Ar.messages_count(100), "100 رسالة");
    }

    #[test]
    fn french_and_english_counts_have_two_forms() {
        assert_eq!(Locale::Fr.messages_count(0), "aucun message");
        assert_eq!(Locale::Fr.messages_count(1), "1 message");
        assert_eq!(Locale::Fr.messages_count(2), "2 messages");
        assert_eq!(Locale::En.messages_count(0), "no messages");
        assert_eq!(Locale::En.messages_count(1), "1 message");
        assert_eq!(Locale::En.messages_count(5), "5 messages");
    }

    #[test]
    fn status_and_engine_tokens_are_translated_not_invented() {
        for status in ["pass", "warn", "fail", "mystery"] {
            for locale in Locale::ALL {
                assert!(!locale.check_status(status).is_empty());
            }
        }
        assert_eq!(Locale::Ar.check_status("pass"), "موجود");
        assert_eq!(Locale::En.check_status("pass"), "present");
        assert_eq!(Locale::Fr.check_status("fail"), "non conforme");

        assert_eq!(Locale::Ar.engine_label("pending"), "قيد التفعيل");
        assert_eq!(Locale::En.engine_label("pending"), "activating");
        assert_eq!(Locale::Fr.engine_label("unknown-state"), "inconnu");
    }

    #[test]
    fn shell_copy_is_free_of_internal_jargon_in_all_locales() {
        // These fields used to name the schema, the hash function, or the
        // engine. A translation must not put the jargon back.
        for locale in Locale::ALL {
            let text = locale.shell();
            for value in [
                text.sessions_help,
                text.password_help,
                text.soon_search_why,
                text.soon_starred_why,
                text.soon_archive_why,
                text.err_hash_failed,
                text.profile_title,
                text.auth_missing,
            ] {
                let lowered = value.to_ascii_lowercase();
                assert!(!lowered.contains("postgres"), "{value}");
                assert!(!lowered.contains("argon2"), "{value}");
                assert!(!lowered.contains("rls"), "{value}");
                assert!(!lowered.contains("stalwart"), "{value}");
                assert!(!lowered.contains("imap"), "{value}");
                assert!(!lowered.contains("sha-256"), "{value}");
                assert!(!lowered.contains("sql"), "{value}");
            }
        }
    }
}
