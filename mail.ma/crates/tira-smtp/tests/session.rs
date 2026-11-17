use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use tira_smtp::message::ParsedMessage;
use tira_smtp::server::{DeliveryError, Envelope, MailHandler, Server, ServerConfig};

#[derive(Clone, Default)]
struct Capture {
    delivered: Arc<Mutex<Vec<(Envelope, ParsedMessage)>>>,
}

#[async_trait]
impl MailHandler for Capture {
    async fn deliver(
        &self,
        envelope: Envelope,
        message: ParsedMessage,
        _raw: Vec<u8>,
    ) -> Result<(), DeliveryError> {
        self.delivered.lock().await.push((envelope, message));
        Ok(())
    }
}

async fn read_reply<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await.expect("read reply");
        assert!(read > 0, "connection closed while waiting for a reply");
        let line = line.trim_end().to_string();
        let more = line.as_bytes().get(3) == Some(&b'-');
        lines.push(line);
        if !more {
            return lines;
        }
    }
}

async fn start_server() -> (std::net::SocketAddr, Capture) {
    let capture = Capture::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let server = Server::new(
        ServerConfig {
            hostname: "tira.test".to_string(),
            session_timeout: Duration::from_secs(5),
            ..ServerConfig::default()
        },
        capture.clone(),
    );
    tokio::spawn(async move {
        let _ = server.serve_on(listener).await;
    });
    (addr, capture)
}

#[tokio::test]
async fn accepts_a_message_end_to_end() {
    let (addr, capture) = start_server().await;
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));

    write_half.write_all(b"EHLO test.local\r\n").await.unwrap();
    let ehlo = read_reply(&mut reader).await;
    assert!(ehlo.iter().any(|l| l.contains("SIZE")));
    assert!(ehlo.last().unwrap().starts_with("250 "));

    write_half
        .write_all(b"MAIL FROM:<aicha@uca.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    write_half
        .write_all(b"RCPT TO:<yassine@tira.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    write_half.write_all(b"DATA\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("354 "));

    write_half
        .write_all(
            b"From: aicha@uca.ma\r\n\
To: yassine@tira.ma\r\n\
Subject: Test\r\n\
\r\n\
Salam, hada test.\r\n\
.\r\n",
        )
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    write_half.write_all(b"QUIT\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("221 "));

    let delivered = capture.delivered.lock().await;
    assert_eq!(delivered.len(), 1, "exactly one message delivered");
    let (envelope, message) = &delivered[0];
    assert_eq!(envelope.mail_from, "aicha@uca.ma");
    assert_eq!(envelope.rcpt_to, vec!["yassine@tira.ma".to_string()]);
    assert_eq!(message.subject.as_deref(), Some("Test"));
    assert!(message
        .body_text
        .as_deref()
        .unwrap_or("")
        .contains("hada test"));
}

#[tokio::test]
async fn rejects_data_before_recipients() {
    let (addr, _capture) = start_server().await;
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    write_half.write_all(b"DATA\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("503 "));
}

#[tokio::test]
async fn refuses_unknown_commands() {
    let (addr, _capture) = start_server().await;
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    write_half.write_all(b"STARTTLS\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("502 "));
}

#[tokio::test]
async fn preserves_leading_dots_in_body() {
    let (addr, capture) = start_server().await;
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    write_half
        .write_all(b"MAIL FROM:<a@uca.ma>\r\nRCPT TO:<b@tira.ma>\r\nDATA\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));
    assert!(read_reply(&mut reader).await[0].starts_with("354 "));

    write_half
        .write_all(b"Subject: dots\r\n\r\n..leading dot\r\n.\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    let delivered = capture.delivered.lock().await;
    assert_eq!(delivered.len(), 1);
    assert!(delivered[0]
        .1
        .body_text
        .as_deref()
        .unwrap_or("")
        .contains(".leading dot"));
}

/// Accepts exactly one address. Used to prove the server refuses unknown
/// recipients at RCPT time instead of becoming an open relay.
struct OnlyKnown;

#[async_trait]
impl MailHandler for OnlyKnown {
    async fn accepts_recipient(&self, address: &str) -> bool {
        address.eq_ignore_ascii_case("yassine@tira.ma")
    }

    async fn deliver(
        &self,
        _envelope: Envelope,
        _message: ParsedMessage,
        _raw: Vec<u8>,
    ) -> Result<(), DeliveryError> {
        Ok(())
    }
}

#[tokio::test]
async fn refuses_unknown_recipient_at_rcpt_time() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let server = Server::new(ServerConfig::default(), OnlyKnown);
    tokio::spawn(async move {
        let _ = server.serve_on(listener).await;
    });

    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    write_half
        .write_all(b"MAIL FROM:<aicha@uca.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    // Unknown recipient: permanent 550, and no message is accepted.
    write_half
        .write_all(b"RCPT TO:<nobody@tira.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("550 "));

    // Known recipient still works in the same transaction.
    write_half
        .write_all(b"RCPT TO:<yassine@tira.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));
}

/// M-18: `MAIL FROM:<>` (the null reverse-path used by DSNs/bounces) must be
/// accepted, and the delivered envelope carries an empty sender. Rejecting it
/// (the old 501) silently lost bounce mail and violated RFC 5321 §4.5.1.
#[tokio::test]
async fn accepts_null_reverse_path_for_bounces() {
    let (addr, capture) = start_server().await;
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    write_half.write_all(b"MAIL FROM:<>\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    write_half
        .write_all(b"RCPT TO:<yassine@tira.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    write_half.write_all(b"DATA\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("354 "));
    write_half
        .write_all(b"Subject: Delivery Status Notification\r\n\r\nbounce\r\n.\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));

    let delivered = capture.delivered.lock().await;
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].0.mail_from, "", "null reverse-path preserved");
}

/// H-04: an over-long command line is refused with 500 and the connection
/// closes, instead of being buffered without limit (the old `read_line` DoS).
#[tokio::test]
async fn refuses_overlong_command_line() {
    let (addr, _capture) = start_server().await;
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    // 4096 bytes, well over the 2048 cap, terminated so the server reads it.
    let mut line = vec![b'A'; 4096];
    line.extend_from_slice(b"\r\n");
    write_half.write_all(&line).await.unwrap();
    let reply = read_reply(&mut reader).await;
    assert!(reply[0].starts_with("500 "), "expected 500, got {reply:?}");
}

/// H-04: an over-long line during DATA is treated as an oversized message
/// (552), not buffered without limit (the old `read_until` DoS that bypassed
/// the SIZE cap).
#[tokio::test]
async fn refuses_overlong_data_line() {
    let (addr, _capture) = start_server().await;
    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    write_half
        .write_all(b"MAIL FROM:<a@uca.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));
    write_half
        .write_all(b"RCPT TO:<b@tira.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));
    write_half.write_all(b"DATA\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("354 "));

    let mut line = vec![b'A'; 4096];
    line.extend_from_slice(b"\r\n");
    write_half.write_all(&line).await.unwrap();
    let reply = read_reply(&mut reader).await;
    assert!(reply[0].starts_with("552 "), "expected 552, got {reply:?}");
}

/// H-04: the DATA phase is bounded by a timeout. A client that starts DATA then
/// stalls (trickle) is cut off with 421 instead of holding a task forever.
#[tokio::test]
async fn data_phase_times_out_on_a_stalled_client() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let server = Server::new(
        ServerConfig {
            hostname: "tira.test".to_string(),
            session_timeout: Duration::from_millis(800),
            ..ServerConfig::default()
        },
        Capture::default(),
    );
    tokio::spawn(async move {
        let _ = server.serve_on(listener).await;
    });

    let stream = TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    assert!(read_reply(&mut reader).await[0].starts_with("220 "));
    write_half
        .write_all(b"MAIL FROM:<a@uca.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));
    write_half
        .write_all(b"RCPT TO:<b@tira.ma>\r\n")
        .await
        .unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("250 "));
    write_half.write_all(b"DATA\r\n").await.unwrap();
    assert!(read_reply(&mut reader).await[0].starts_with("354 "));

    // A partial line, then silence. The 800ms DATA timeout must fire.
    write_half.write_all(b"Subject: stalled").await.unwrap();
    let reply = read_reply(&mut reader).await;
    assert!(reply[0].starts_with("421 "), "expected 421, got {reply:?}");
}

/// H-04: concurrent sessions are capped (MAX_SESSIONS = 256 in server.rs). A
/// connection beyond the limit is dropped without a greeting rather than
/// queued or served. The held sessions read their 220 greeting, which is only
/// sent after a permit is acquired, so all 256 permits are provably held before
/// the 257th connection is attempted.
#[tokio::test]
async fn caps_concurrent_sessions() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let server = Server::new(
        ServerConfig {
            hostname: "tira.test".to_string(),
            // Long enough that held sessions cannot time out mid-test.
            session_timeout: Duration::from_secs(30),
            ..ServerConfig::default()
        },
        Capture::default(),
    );
    tokio::spawn(async move {
        let _ = server.serve_on(listener).await;
    });

    let mut held = Vec::new();
    for _ in 0..256 {
        let stream = TcpStream::connect(addr).await.expect("connect");
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut greeting = String::new();
        reader.read_line(&mut greeting).await.expect("greeting");
        assert!(greeting.starts_with("220 "));
        held.push((write_half, reader));
    }

    let mut excess = TcpStream::connect(addr).await.expect("connect excess");
    let mut buf = [0u8; 64];
    match excess.read(&mut buf).await {
        Ok(0) => {} // clean EOF: dropped at the cap
        Ok(n) => panic!("excess connection served {n} bytes; cap not enforced"),
        Err(_) => {} // connection reset: also dropped
    }
    drop(held);
}
