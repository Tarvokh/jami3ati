//! Tira Mail persistence.
//!
//! Two independent halves, joined by [`PgStore`]:
//!
//! - [`blob`]: content-addressed storage for message bytes (filesystem now,
//!   S3/MinIO later). Deduplicates and detects corruption.
//! - [`store`]: PostgreSQL metadata — tenants, domains, mailboxes, messages,
//!   audit log — and the [`tira_smtp::server::MailHandler`] implementation
//!   that turns the SMTP server from a demo into something that remembers.
//!
//! What this crate deliberately does **not** do yet: IMAP, full-text search,
//! attachment extraction, and S3. See `docs/DEEP_MAP.ar.md` §5.

pub mod blob;
pub mod db;
pub mod identity;
pub mod model;
pub mod proof;
pub mod provision;
pub mod scoped;
pub mod store;
pub mod token;

#[cfg(test)]
mod source_hygiene;

pub use blob::{BlobError, BlobRef, BlobStore, FsBlobStore};
pub use identity::{LoginCredential, MailboxProfile, MailboxSession};
pub use model::{Domain, Mailbox, OutboundQueueEntry, StoredMessage, Tenant};
pub use proof::{AuditProof, CrownJewelProof, LiveProof};
pub use provision::{EngineState, PendingMailbox};
pub use scoped::{AuditContext, AuditEntry, MailboxPage};
pub use store::{PgStore, StoreError};
pub use token::{ApiToken, IssuedToken, TOKEN_PREFIX};

pub const NAME: &str = "tira-store";
