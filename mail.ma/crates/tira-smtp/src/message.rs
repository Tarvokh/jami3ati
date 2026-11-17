//! RFC 5322 / MIME parsing.
//!
//! The SMTP server receives raw bytes. Everything downstream (storage,
//! indexing, hooks) works on [`ParsedMessage`], never on raw bytes.

use mail_parser::MessageParser;

/// A message after MIME decoding, reduced to what Tira actually needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub message_id: Option<String>,
    pub subject: Option<String>,
    pub from: Vec<String>,
    pub to: Vec<String>,
    /// Unix timestamp from the `Date:` header, if present and valid.
    pub date: Option<i64>,
    pub body_text: Option<String>,
    pub size_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("message is empty")]
    Empty,
    #[error("message is not valid MIME / RFC 5322")]
    Malformed,
}

/// Parses raw message bytes. Never panics on hostile input.
pub fn parse(raw: &[u8]) -> Result<ParsedMessage, ParseError> {
    if raw.is_empty() {
        return Err(ParseError::Empty);
    }

    let parsed = MessageParser::default()
        .parse(raw)
        .ok_or(ParseError::Malformed)?;

    Ok(ParsedMessage {
        message_id: parsed.message_id().map(str::to_owned),
        subject: parsed.subject().map(str::to_owned),
        from: parsed.from().map(collect_addresses).unwrap_or_default(),
        to: parsed.to().map(collect_addresses).unwrap_or_default(),
        date: parsed.date().map(|d| d.to_timestamp()),
        body_text: parsed.body_text(0).map(|c| c.into_owned()),
        size_bytes: raw.len(),
    })
}

fn collect_addresses(address: &mail_parser::Address) -> Vec<String> {
    address
        .iter()
        .filter_map(|addr| addr.address().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"From: Aicha <aicha@uca.ma>\r\n\
To: Yassine <yassine@tira.ma>\r\n\
Subject: Tira test\r\n\
Date: Tue, 03 Nov 2026 10:00:00 +0000\r\n\
Message-ID: <abc123@uca.ma>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Salam, hada test.\r\n";

    #[test]
    fn parses_envelope_and_body() {
        let msg = parse(SAMPLE).expect("sample must parse");
        assert_eq!(msg.from, vec!["aicha@uca.ma"]);
        assert_eq!(msg.to, vec!["yassine@tira.ma"]);
        assert_eq!(msg.subject.as_deref(), Some("Tira test"));
        // `mail-parser` strips the RFC 5322 angle brackets for us.
        assert_eq!(msg.message_id.as_deref(), Some("abc123@uca.ma"));
        assert!(msg.body_text.as_deref().unwrap_or("").contains("hada test"));
        assert_eq!(msg.size_bytes, SAMPLE.len());
    }

    #[test]
    fn rejects_empty_and_garbage() {
        assert!(matches!(parse(b""), Err(ParseError::Empty)));
        // Binary noise with no headers must not panic.
        let _ = parse(&[0xff, 0x00, 0x13, 0x37]);
    }
}
