//! Tira Mail — SMTP layer.
//!
//! Two halves:
//! - [`server`]: an inbound ESMTP server (tokio) that accepts mail and hands
//!   parsed messages to a [`server::MailHandler`].
//! - [`client`]: an outbound submission client (lettre) used to relay mail
//!   through the sovereign relay.
//! - [`message`]: RFC 5322 / MIME parsing shared by both halves.

pub mod client;
pub mod message;
pub mod server;

pub const NAME: &str = "tira-smtp";
