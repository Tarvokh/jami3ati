//! Inbound ESMTP server.
//!
//! Deliberately small: EHLO/HELO, MAIL, RCPT, DATA, RSET, NOOP, QUIT.
//! Everything else is refused with 502 instead of silently accepted.
//!
//! What this is NOT (yet): STARTTLS, AUTH, PIPELINING, CHUNKING, and spam
//! filtering. Those belong to the submission path and the filtering layer,
//! and pretending they exist would be worse than not having them.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use crate::message::{self, ParsedMessage};

/// H-04: RFC 5321 §4.5.3.1 bounds command lines (512 octets) and message text
/// lines (1000 octets). We allow generous headroom for extensions, but the
/// buffer is hard-capped so an unterminated line cannot exhaust memory. The
/// old `read_line`/`read_until` grew without limit until a newline arrived, so
/// a multi-gigabyte line with no newline bypassed the SIZE cap entirely.
const MAX_COMMAND_LINE: usize = 2048;
const MAX_DATA_LINE: usize = 2048;

/// H-04: cap concurrent sessions so a connection flood cannot exhaust tasks
/// and memory. Excess connections are accepted then dropped immediately
/// (backpressure would queue unbounded waiters, which is the same DoS).
const MAX_SESSIONS: usize = 256;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Name announced in the 220 greeting and EHLO response.
    pub hostname: String,
    /// Hard cap on a single message body.
    pub max_message_bytes: usize,
    /// Hard cap on recipients per transaction.
    pub max_recipients: usize,
    /// Idle timeout per command.
    pub session_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: "tira.local".to_string(),
            max_message_bytes: 10 * 1024 * 1024,
            max_recipients: 50,
            session_timeout: Duration::from_secs(300),
        }
    }
}

/// The SMTP envelope, independent of message content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Envelope {
    pub mail_from: String,
    pub rcpt_to: Vec<String>,
}

/// Why a delivery attempt failed. The variant decides the SMTP reply code,
/// because "try again later" and "no such user" must not be conflated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    /// 550: this server does not handle mail for that address. Permanent.
    UnknownRecipient(String),
    /// 451: transient failure (disk full, database down). Retry later.
    Temporary(String),
    /// 552: the mailbox has no room left.
    OverQuota(String),
}

impl std::fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeliveryError::UnknownRecipient(addr) => {
                write!(f, "unknown recipient: {addr}")
            }
            DeliveryError::Temporary(reason) => write!(f, "temporary failure: {reason}"),
            DeliveryError::OverQuota(address) => write!(f, "quota exceeded for {address}"),
        }
    }
}

impl std::error::Error for DeliveryError {}

/// Where accepted mail goes. Implemented by the storage/indexing layer.
#[async_trait]
pub trait MailHandler: Send + Sync + 'static {
    /// Called at RCPT time. Returning `false` makes the server answer
    /// `550 5.1.1`, which is how an open relay is avoided: mail for an
    /// unknown address is refused instead of accepted and bounced.
    ///
    /// The default accepts everything, which is only safe for tests.
    async fn accepts_recipient(&self, _address: &str) -> bool {
        true
    }

    /// Called once per message, after DATA is complete.
    async fn deliver(
        &self,
        envelope: Envelope,
        message: ParsedMessage,
        raw: Vec<u8>,
    ) -> Result<(), DeliveryError>;
}

pub struct Server<H: MailHandler> {
    config: ServerConfig,
    handler: Arc<H>,
}

impl<H: MailHandler> Server<H> {
    pub fn new(config: ServerConfig, handler: H) -> Self {
        Self {
            config,
            handler: Arc::new(handler),
        }
    }

    /// Binds `addr` and serves forever.
    pub async fn serve(self, addr: SocketAddr) -> io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        self.serve_on(listener).await
    }

    /// Serves on an already-bound listener (used by tests with port 0).
    pub async fn serve_on(self, listener: TcpListener) -> io::Result<()> {
        // H-04: bound concurrency. The permit is moved into the session task
        // and released when it ends, so at most MAX_SESSIONS sessions run at
        // once. A flood beyond that is dropped, not queued.
        let limit = Arc::new(Semaphore::new(MAX_SESSIONS));
        loop {
            let (stream, peer) = listener.accept().await?;
            let permit = match limit.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    tracing::warn!(%peer, "smtp session limit reached; dropping connection");
                    drop(stream);
                    continue;
                }
            };
            let config = self.config.clone();
            let handler = Arc::clone(&self.handler);
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(err) = run_session(stream, config, handler).await {
                    tracing::debug!(%peer, error = %err, "smtp session ended with error");
                }
            });
        }
    }
}

#[derive(Default)]
struct SessionState {
    mail_from: Option<String>,
    rcpt_to: Vec<String>,
}

impl SessionState {
    fn reset(&mut self) {
        self.mail_from = None;
        self.rcpt_to.clear();
    }
}

enum DataOutcome {
    Complete(Vec<u8>),
    TooLarge,
    Closed,
}

async fn run_session<H: MailHandler>(
    stream: TcpStream,
    config: ServerConfig,
    handler: Arc<H>,
) -> io::Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut state = SessionState::default();

    write_reply(
        &mut write_half,
        &format!("220 {} ESMTP Tira Mail", config.hostname),
    )
    .await?;

    loop {
        // H-04: bounded line read. An over-long line is refused with 500 and
        // the session closes, instead of buffering it without limit.
        let line_buf = match timeout(
            config.session_timeout,
            read_line_capped(&mut reader, MAX_COMMAND_LINE),
        )
        .await
        {
            Ok(Ok(Some(bytes))) => bytes,
            Ok(Ok(None)) => return Ok(()),
            Ok(Err(err)) => {
                if err.kind() == io::ErrorKind::InvalidData {
                    let _ = write_reply(&mut write_half, "500 5.5.1 Line too long").await;
                    return Ok(());
                }
                return Err(err);
            }
            Err(_) => {
                write_reply(&mut write_half, "421 4.4.2 Idle timeout").await?;
                return Ok(());
            }
        };
        let command = match std::str::from_utf8(&line_buf) {
            Ok(text) => text.trim_end_matches(['\r', '\n']),
            Err(_) => {
                write_reply(&mut write_half, "500 5.5.2 Invalid command encoding").await?;
                continue;
            }
        };
        let (verb, rest) = split_command(command);

        match verb.as_str() {
            "EHLO" => {
                let caps = format!(
                    "250-{}\r\n250-SIZE {}\r\n250-8BITMIME\r\n250 HELP",
                    config.hostname, config.max_message_bytes
                );
                write_reply(&mut write_half, &caps).await?;
            }
            "HELO" => {
                write_reply(&mut write_half, &format!("250 {}", config.hostname)).await?;
            }
            "MAIL" => {
                if !rest.to_ascii_uppercase().starts_with("FROM:") {
                    write_reply(&mut write_half, "501 5.5.2 Expected MAIL FROM:<addr>").await?;
                    continue;
                }
                match angle_addr(&rest[5..], true) {
                    Some(addr) => {
                        state.reset();
                        state.mail_from = Some(addr);
                        write_reply(&mut write_half, "250 2.1.0 Sender accepted").await?;
                    }
                    None => {
                        write_reply(&mut write_half, "501 5.1.7 Bad sender address").await?;
                    }
                }
            }
            "RCPT" => {
                if state.mail_from.is_none() {
                    write_reply(&mut write_half, "503 5.5.1 Send MAIL FROM first").await?;
                    continue;
                }
                if !rest.to_ascii_uppercase().starts_with("TO:") {
                    write_reply(&mut write_half, "501 5.5.2 Expected RCPT TO:<addr>").await?;
                    continue;
                }
                if state.rcpt_to.len() >= config.max_recipients {
                    write_reply(&mut write_half, "452 4.5.3 Too many recipients").await?;
                    continue;
                }
                match angle_addr(&rest[3..], false) {
                    Some(addr) => {
                        if !handler.accepts_recipient(&addr).await {
                            write_reply(&mut write_half, "550 5.1.1 No such recipient here")
                                .await?;
                            continue;
                        }
                        state.rcpt_to.push(addr);
                        write_reply(&mut write_half, "250 2.1.5 Recipient accepted").await?;
                    }
                    None => {
                        write_reply(&mut write_half, "501 5.1.3 Bad recipient address").await?;
                    }
                }
            }
            "DATA" => {
                if state.mail_from.is_none() || state.rcpt_to.is_empty() {
                    write_reply(&mut write_half, "503 5.5.1 Need MAIL FROM and RCPT TO").await?;
                    continue;
                }
                write_reply(&mut write_half, "354 End data with <CR><LF>.<CR><LF>").await?;

                // H-04: the per-command idle timeout does not cover DATA. Cap
                // the whole transfer so a trickle client (a byte every few
                // seconds) cannot hold a task forever.
                let outcome = match timeout(
                    config.session_timeout,
                    read_data(&mut reader, config.max_message_bytes),
                )
                .await
                {
                    Ok(inner) => inner?,
                    Err(_) => {
                        let _ =
                            write_reply(&mut write_half, "421 4.4.2 DATA transfer timeout").await;
                        return Ok(());
                    }
                };
                match outcome {
                    DataOutcome::Complete(raw) => {
                        let envelope = Envelope {
                            mail_from: state.mail_from.clone().unwrap_or_default(),
                            rcpt_to: state.rcpt_to.clone(),
                        };
                        state.reset();

                        match message::parse(&raw) {
                            Ok(parsed) => match handler.deliver(envelope, parsed, raw).await {
                                Ok(()) => {
                                    write_reply(
                                        &mut write_half,
                                        "250 2.0.0 Message accepted for delivery",
                                    )
                                    .await?;
                                }
                                Err(DeliveryError::UnknownRecipient(addr)) => {
                                    tracing::warn!(%addr, "recipient vanished before delivery");
                                    write_reply(
                                        &mut write_half,
                                        "550 5.1.1 No such recipient here",
                                    )
                                    .await?;
                                }
                                Err(DeliveryError::Temporary(reason)) => {
                                    tracing::warn!(%reason, "delivery failed, answering 451");
                                    write_reply(&mut write_half, "451 4.3.0 Try again later")
                                        .await?;
                                }
                                Err(DeliveryError::OverQuota(address)) => {
                                    tracing::warn!(%address, "mailbox over quota, answering 552");
                                    write_reply(
                                        &mut write_half,
                                        "552 5.2.2 Mailbox full, message too large for quota",
                                    )
                                    .await?;
                                }
                            },
                            Err(err) => {
                                tracing::warn!(%err, "rejected unparsable message");
                                write_reply(&mut write_half, "554 5.6.0 Invalid message content")
                                    .await?;
                            }
                        }
                    }
                    DataOutcome::TooLarge => {
                        write_reply(&mut write_half, "552 5.3.4 Message too large").await?;
                        return Ok(());
                    }
                    DataOutcome::Closed => return Ok(()),
                }
            }
            "RSET" => {
                state.reset();
                write_reply(&mut write_half, "250 2.0.0 Reset").await?;
            }
            "NOOP" => {
                write_reply(&mut write_half, "250 2.0.0 Ok").await?;
            }
            "QUIT" => {
                write_reply(&mut write_half, "221 2.0.0 Bye").await?;
                return Ok(());
            }
            "" => {
                write_reply(&mut write_half, "500 5.5.2 Empty command").await?;
            }
            _ => {
                write_reply(&mut write_half, "502 5.5.2 Command not implemented").await?;
            }
        }
    }
}

/// Splits `MAIL FROM:<a@b>` into ("MAIL", "FROM:<a@b>").
fn split_command(command: &str) -> (String, String) {
    let command = command.trim();
    match command.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb.to_ascii_uppercase(), rest.trim().to_string()),
        None => (command.to_ascii_uppercase(), String::new()),
    }
}

/// Extracts `addr` from `<addr>` or `addr`.
///
/// M-18: `allow_null` permits the empty reverse-path `<>` (RFC 5321 §4.5.1),
/// which is how DSNs/bounces are sent. It is valid only for MAIL FROM and only
/// inside explicit angles; RCPT TO must still reject it (you cannot deliver to
/// nobody), and a bare empty argument is never a null path.
fn angle_addr(input: &str, allow_null: bool) -> Option<String> {
    let input = input.trim();
    let (candidate, had_angles) = if let Some(start) = input.find('<') {
        let end = input[start..].find('>')? + start;
        (&input[start + 1..end], true)
    } else {
        (input.split_whitespace().next().unwrap_or(""), false)
    };

    let candidate = candidate.trim();
    if candidate.is_empty() {
        return if allow_null && had_angles {
            Some(String::new())
        } else {
            None
        };
    }
    if !candidate.contains('@') || candidate.contains(char::is_whitespace) {
        return None;
    }
    Some(candidate.to_string())
}

/// H-04: reads one `\n`-terminated line without ever buffering more than
/// `max_len` bytes. Returns `Ok(None)` at EOF before any byte, `Ok(Some(line))`
/// otherwise (the trailing `\n` is included when present; a final unterminated
/// line is returned as-is). Returns `InvalidData` the moment the line would
/// exceed `max_len`, so a peer cannot exhaust memory by sending an unterminated
/// megabyte/gigabyte line — the failure mode of the old `read_until`.
async fn read_line_capped<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    max_len: usize,
) -> io::Result<Option<Vec<u8>>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(if buf.is_empty() { None } else { Some(buf) });
        }
        let (take, done) = match available.iter().position(|&b| b == b'\n') {
            Some(pos) => (pos + 1, true),
            None => (available.len(), false),
        };
        if buf.len() + take > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "line exceeds maximum length",
            ));
        }
        buf.extend_from_slice(&available[..take]);
        reader.consume(take);
        if done {
            return Ok(Some(buf));
        }
    }
}

async fn read_data<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    max_bytes: usize,
) -> io::Result<DataOutcome> {
    let mut data = Vec::new();

    loop {
        // H-04: each DATA line is bounded too. An over-long line is treated as
        // an oversized message (552) rather than buffered without limit.
        let line = match read_line_capped(reader, MAX_DATA_LINE).await {
            Ok(Some(line)) => line,
            Ok(None) => return Ok(DataOutcome::Closed),
            Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                return Ok(DataOutcome::TooLarge);
            }
            Err(err) => return Err(err),
        };
        if line == b".\r\n" || line == b".\n" {
            return Ok(DataOutcome::Complete(data));
        }

        // RFC 5321 4.5.2: a leading dot is doubled by the sender.
        let payload: &[u8] = if line.starts_with(b"..") {
            &line[1..]
        } else {
            &line[..]
        };

        if data.len() + payload.len() > max_bytes {
            return Ok(DataOutcome::TooLarge);
        }
        data.extend_from_slice(payload);
    }
}

async fn write_reply<W: AsyncWriteExt + Unpin>(writer: &mut W, reply: &str) -> io::Result<()> {
    writer.write_all(reply.as_bytes()).await?;
    writer.write_all(b"\r\n").await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_paths() {
        assert_eq!(angle_addr("<a@b.ma>", false).as_deref(), Some("a@b.ma"));
        assert_eq!(
            angle_addr("<a@b.ma> SIZE=100", false).as_deref(),
            Some("a@b.ma")
        );
        assert_eq!(angle_addr("a@b.ma", false).as_deref(), Some("a@b.ma"));
        assert_eq!(angle_addr("<>", false), None);
        assert_eq!(angle_addr("<nope>", false), None);
        assert_eq!(angle_addr("", false), None);
    }

    /// M-18: the null reverse-path `<>` is valid for MAIL FROM (DSN/bounce)
    /// but never for RCPT TO, and a bare empty argument is not a null path.
    #[test]
    fn null_reverse_path_only_for_mail_from() {
        assert_eq!(angle_addr("<>", true).as_deref(), Some(""));
        assert_eq!(angle_addr("<>", false), None);
        assert_eq!(angle_addr("", true), None);
        assert_eq!(angle_addr("   ", true), None);
        assert_eq!(angle_addr("<a@b.ma>", true).as_deref(), Some("a@b.ma"));
    }

    /// H-04: the bounded reader returns a normal line, signals EOF, and refuses
    /// an over-long line with InvalidData instead of buffering it without limit.
    #[tokio::test]
    async fn read_line_capped_bounds_the_buffer() {
        let data = b"hello\r\nworld\r\n";
        let mut reader = BufReader::new(&data[..]);
        let first = read_line_capped(&mut reader, 64).await.unwrap();
        assert_eq!(first, Some(b"hello\r\n".to_vec()));
        let second = read_line_capped(&mut reader, 64).await.unwrap();
        assert_eq!(second, Some(b"world\r\n".to_vec()));
        let eof = read_line_capped(&mut reader, 64).await.unwrap();
        assert_eq!(eof, None);

        // A final unterminated line is returned, then EOF.
        let tail = b"tail";
        let mut reader = BufReader::new(&tail[..]);
        let last = read_line_capped(&mut reader, 64).await.unwrap();
        assert_eq!(last, Some(b"tail".to_vec()));

        // An over-long line is refused, not buffered.
        let long = vec![b'A'; 4096];
        let mut reader = BufReader::new(&long[..]);
        let err = read_line_capped(&mut reader, 2048).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        // Exactly at the cap is allowed.
        let exact = vec![b'B'; 2048];
        let mut reader = BufReader::new(&exact[..]);
        let fits = read_line_capped(&mut reader, 2048).await.unwrap();
        assert_eq!(fits, Some(vec![b'B'; 2048]));
    }

    #[test]
    fn splits_commands() {
        let (verb, rest) = split_command("mail from:<a@b.ma>");
        assert_eq!(verb, "MAIL");
        assert_eq!(rest, "from:<a@b.ma>");
        assert_eq!(split_command("QUIT"), ("QUIT".to_string(), String::new()));
    }
}
