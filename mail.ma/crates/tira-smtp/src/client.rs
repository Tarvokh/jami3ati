//! Outbound SMTP submission client.
//!
//! Uses `lettre` rather than a hand-rolled client: submission needs STARTTLS,
//! AUTH, connection pooling and error classification that a from-scratch
//! implementation gets wrong.

use std::fmt;
use std::net::IpAddr;

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("invalid email address: {0}")]
    Address(String),
    #[error("invalid message: {0}")]
    Message(String),
    #[error("smtp transport: {0}")]
    Transport(String),
    #[error("smtp send: {0}")]
    Send(String),
}

pub struct SmtpClient {
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpClient {
    /// Submission with mandatory STARTTLS and credentials.
    pub fn starttls(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<Self, MailError> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|err| MailError::Transport(err.to_string()))?
            .port(port)
            .credentials(Credentials::new(username.to_string(), password.to_string()))
            .build();
        Ok(Self { transport })
    }

    /// Plaintext submission. Only for a loopback relay during development.
    ///
    /// Fails closed for any non-loopback host: submitting credentials without
    /// TLS to a routable host is never acceptable, even if the operator set
    /// `TIRA_SMTP_STARTTLS=0`. That opt-in exists for the local manual
    /// `tira-smtp` server only; a remote relay must use [`SmtpClient::starttls`].
    pub fn plaintext(host: &str, port: u16) -> Result<Self, MailError> {
        if !is_loopback_host(host) {
            return Err(MailError::Transport(format!(
                "plaintext SMTP refused for non-loopback host '{host}'; use STARTTLS (TIRA_SMTP_STARTTLS=1) for remote relays"
            )));
        }
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .build();
        Ok(Self { transport })
    }

    pub async fn send(&self, message: Message) -> Result<(), MailError> {
        self.transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(|err| MailError::Send(err.to_string()))
    }
}

/// Outbound submission relay configuration, sourced from the operator
/// environment.
///
/// `None` means the process runs without a configured relay, and the send
/// surface then refuses to build a client instead of silently submitting
/// through an unintended host. STARTTLS is mandatory for real relays; the
/// manual loopback `tira-smtp` server is plaintext on purpose during local
/// development, so `TIRA_SMTP_STARTTLS=0` opts into that path only.
#[derive(Clone)]
pub struct SmtpRelay {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub starttls: bool,
}

/// The relay password must never reach a log or a `{:?}` dump. Host/port/
/// username/starttls are safe diagnostics; the password is masked.
impl fmt::Debug for SmtpRelay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpRelay")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("starttls", &self.starttls)
            .finish()
    }
}

impl SmtpRelay {
    pub fn from_env() -> Option<Self> {
        let host = env_non_empty("TIRA_SMTP_HOST")?;
        let port = std::env::var("TIRA_SMTP_PORT")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(587);
        let username = env_non_empty("TIRA_SMTP_USER").unwrap_or_default();
        let password = env_non_empty("TIRA_SMTP_PASS").unwrap_or_default();
        let starttls = match std::env::var("TIRA_SMTP_STARTTLS") {
            Ok(value) => !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "0" | "false" | "no" | "off"
            ),
            Err(_) => true,
        };
        Some(Self {
            host,
            port,
            username,
            password,
            starttls,
        })
    }
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// True only for the loopback literals: `localhost` (any case) or an IP in
/// `127.0.0.0/8` / `::1`. A routable IP or any other name is false, so plaintext
/// submission can never be aimed at a remote host. Mirrors the PowerShell
/// `Test-LoopbackHost` guard used by the proof scripts.
fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Parses a single address without building a message, so a caller can
/// validate an optional `Reply-To` before it reaches the durable queue.
pub fn validate_address(addr: &str) -> Result<(), MailError> {
    addr.parse::<Mailbox>()
        .map(|_| ())
        .map_err(|_| MailError::Address(addr.to_string()))
}

/// Builds a UTF-8 text message with optional extra headers.
///
/// Header *names* are `&'static str` on purpose: the caller can only choose
/// from literals it wrote itself, so an untrusted string can never become a
/// header name. Values are still untrusted, so they are constrained to
/// printable ASCII with no CR/LF — a header value is otherwise a classic
/// injection surface.
///
/// `lettre` folds at spaces only, so a long unbroken URL (the usual
/// `List-Unsubscribe` shape) is written on one line rather than split.
///
/// The message always declares `MIME-Version: 1.0` and
/// `Content-Type: text/plain; charset=utf-8`. `MessageBuilder::body()` sets
/// only `Content-Transfer-Encoding` (base64 for non-ASCII text), so without
/// those two headers RFC 2045's default — `text/plain; charset=us-ascii` —
/// applies and a strict client decodes the base64 payload as US-ASCII. Arabic
/// is the whole point of this product, so the charset is never left implicit.
/// Callers may not override these three content headers.
pub fn build_message_with(
    from: &str,
    to: &[&str],
    subject: &str,
    body: &str,
    extra_headers: &[(&'static str, &str)],
) -> Result<Message, MailError> {
    if to.is_empty() {
        return Err(MailError::Address("no recipients".to_string()));
    }

    let from: Mailbox = from
        .parse()
        .map_err(|_| MailError::Address(from.to_string()))?;

    let mut builder = Message::builder().from(from).subject(subject);
    for recipient in to {
        let mailbox: Mailbox = recipient
            .parse()
            .map_err(|_| MailError::Address((*recipient).to_string()))?;
        builder = builder.to(mailbox);
    }

    for (name, value) in extra_headers {
        if name.is_empty()
            || name.len() > 76
            || !name.is_ascii()
            || name.bytes().any(|byte| byte == b' ' || byte == b':')
        {
            return Err(MailError::Message(format!("invalid header name '{name}'")));
        }
        if is_reserved_content_header(name) {
            return Err(MailError::Message(format!(
                "header '{name}' is set by the message builder and cannot be overridden"
            )));
        }
        if value.is_empty() || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
            return Err(MailError::Message(format!(
                "header '{name}' contains characters that are not printable ASCII"
            )));
        }
        builder = builder.raw_header(literal_header(name, (*value).to_string()));
    }

    // `body()` writes the encoded payload and `Content-Transfer-Encoding`, but
    // never the two headers that make that encoding interpretable. Order does
    // not matter to parsers; presence does.
    builder = builder
        .raw_header(literal_header("MIME-Version", "1.0".to_string()))
        .raw_header(literal_header(
            "Content-Type",
            "text/plain; charset=utf-8".to_string(),
        ));

    builder
        .body(body.to_string())
        .map_err(|err| MailError::Message(err.to_string()))
}

/// Header names this builder owns. A caller that could set them could ship a
/// message whose declared charset contradicts its bytes.
fn is_reserved_content_header(name: &str) -> bool {
    ["MIME-Version", "Content-Type", "Content-Transfer-Encoding"]
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

/// A raw header built from a literal name. The value is caller-checked
/// elsewhere; this helper exists so the fixed content headers and the optional
/// ones go through the same construction path.
fn literal_header(name: &'static str, value: String) -> lettre::message::header::HeaderValue {
    lettre::message::header::HeaderValue::new(
        lettre::message::header::HeaderName::new_from_ascii_str(name),
        value,
    )
}

/// Builds a UTF-8 text message. Returns an error instead of panicking on
/// malformed addresses, because addresses come from user input.
pub fn build_message(
    from: &str,
    to: &[&str],
    subject: &str,
    body: &str,
) -> Result<Message, MailError> {
    build_message_with(from, to, subject, body, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn builds_message_with_multiple_recipients() {
        let msg = build_message(
            "aicha@uca.ma",
            &["yassine@tira.ma", "salma@tira.ma"],
            "Test",
            "Salam",
        )
        .expect("message must build");
        let raw = String::from_utf8(msg.formatted()).expect("headers are ascii");
        assert!(raw.contains("aicha@uca.ma"));
        assert!(raw.contains("yassine@tira.ma"));
        assert!(raw.contains("salma@tira.ma"));
    }

    #[test]
    fn rejects_bad_addresses() {
        assert!(build_message("not-an-email", &["y@tira.ma"], "s", "b").is_err());
        assert!(build_message("a@uca.ma", &[], "s", "b").is_err());
    }

    #[test]
    fn extra_headers_are_emitted_verbatim() {
        let msg = build_message_with(
            "no-reply@tira.test",
            &["etudiant@example.ma"],
            "تحديث موعد",
            "نص الرسالة",
            &[
                (
                    "X-Jami3ati-Notification",
                    "6f1c2a4e-1111-4222-8333-444455556666",
                ),
                ("List-Unsubscribe", "<https://jami3ati.ma/u/abc.def>"),
                ("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"),
                ("Reply-To", "no-reply@jami3ati.ma"),
            ],
        )
        .expect("message must build");
        let raw = String::from_utf8(msg.formatted()).expect("headers are ascii");
        assert!(
            raw.contains("X-Jami3ati-Notification: 6f1c2a4e-1111-4222-8333-444455556666"),
            "raw was: {raw}"
        );
        assert!(
            raw.contains("List-Unsubscribe: <https://jami3ati.ma/u/abc.def>"),
            "raw was: {raw}"
        );
        assert!(
            raw.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click"),
            "raw was: {raw}"
        );
        assert!(
            raw.contains("Reply-To: no-reply@jami3ati.ma"),
            "raw was: {raw}"
        );
    }

    /// The wire bytes, split at the blank line that ends the header block.
    fn split_message(raw: &[u8]) -> (String, String) {
        let pos = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("every message has a header/body separator");
        (
            String::from_utf8_lossy(&raw[..pos]).to_string(),
            String::from_utf8_lossy(&raw[pos + 4..]).to_string(),
        )
    }

    #[test]
    fn declares_mime_version_and_utf8_charset_so_arabic_survives_the_wire() {
        let body = "أعلنت المدرسة فتح باب الترشيح حتى 30 نوفمبر 2026.";
        let msg = build_message(
            "no-reply@tira.test",
            &["etudiant@example.ma"],
            "تحديث موعد",
            body,
        )
        .expect("message must build");
        let raw = msg.formatted();
        let (head, payload) = split_message(&raw);

        assert!(head.contains("MIME-Version: 1.0"), "head was: {head}");
        assert!(
            head.contains("Content-Type: text/plain; charset=utf-8"),
            "head was: {head}"
        );
        assert!(
            head.contains("Content-Transfer-Encoding: base64"),
            "head was: {head}"
        );

        // The declared charset must describe the bytes that follow it, not just
        // be present. Decoding is the only assertion that proves that.
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload.replace(['\r', '\n'], ""))
            .expect("payload is valid base64");
        assert_eq!(
            String::from_utf8(decoded).expect("payload is utf-8"),
            body,
            "the decoded body must be the text that was handed in"
        );
    }

    #[test]
    fn rejects_attempts_to_override_the_content_headers() {
        for name in [
            "Content-Type",
            "content-type",
            "MIME-Version",
            "Content-Transfer-Encoding",
        ] {
            let result = build_message_with(
                "a@tira.test",
                &["b@tira.test"],
                "s",
                "b",
                &[(name, "text/html; charset=iso-8859-1")],
            );
            assert!(
                matches!(result, Err(MailError::Message(_))),
                "expected a refusal for {name:?}"
            );
        }
    }

    #[test]
    fn rejects_header_injection_and_control_bytes() {
        for hostile in [
            "ok\r\nBcc: evil@example.com",
            "ok\nBcc: evil@example.com",
            "قيمة عربية",
            "tab\there",
        ] {
            let result = build_message_with(
                "a@tira.test",
                &["b@tira.test"],
                "s",
                "b",
                &[("X-Test", hostile)],
            );
            assert!(
                matches!(result, Err(MailError::Message(_))),
                "expected a refusal for {hostile:?}"
            );
        }
    }

    #[test]
    fn smtp_relay_debug_never_prints_the_password() {
        let relay = SmtpRelay {
            host: "smtp.example.com".to_string(),
            port: 587,
            username: "mailer".to_string(),
            password: "super-secret-relay-pw".to_string(),
            starttls: true,
        };
        let rendered = format!("{relay:?}");
        assert!(
            !rendered.contains("super-secret-relay-pw"),
            "Debug leaked the password: {rendered}"
        );
        assert!(
            rendered.contains("<redacted>"),
            "password must be masked: {rendered}"
        );
        assert!(
            rendered.contains("smtp.example.com"),
            "host stays diagnosable: {rendered}"
        );
    }

    // Building a lettre transport spawns its connection-pool task, which needs
    // a Tokio runtime; hence tokio::test rather than a plain #[test].
    #[tokio::test]
    async fn plaintext_submission_is_loopback_only() {
        // Loopback literals build a client without connecting.
        for host in ["127.0.0.1", "127.0.0.2", "::1", "localhost", "LOCALHOST"] {
            assert!(
                SmtpClient::plaintext(host, 2525).is_ok(),
                "loopback '{host}' must be allowed"
            );
        }
        // Anything routable is refused before a transport is built.
        for host in [
            "8.8.8.8",
            "192.168.1.10",
            "10.0.0.1",
            "mail.example.com",
            "::2",
        ] {
            assert!(
                matches!(
                    SmtpClient::plaintext(host, 587),
                    Err(MailError::Transport(_))
                ),
                "expected a Transport refusal for '{host}'"
            );
        }
    }

    #[test]
    fn loopback_classification_is_strict() {
        for host in ["127.0.0.1", "::1", "[::1]", "localhost", " 127.0.0.1 "] {
            assert!(is_loopback_host(host), "expected loopback: '{host}'");
        }
        for host in [
            "8.8.8.8",
            "192.168.1.10",
            "mail.tira.test",
            "example.com",
            "::2",
            "",
        ] {
            assert!(!is_loopback_host(host), "expected NOT loopback: '{host}'");
        }
    }
}
