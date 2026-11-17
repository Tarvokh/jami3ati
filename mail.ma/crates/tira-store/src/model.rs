//! Plain data types returned by the store. No behaviour, no SQL.

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    /// Reference to the CNDP filing for this tenant, if any. See
    /// `docs/DEEP_MAP.ar.md` §11.
    pub cndp_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    pub id: Uuid,
    pub tenant_id: Uuid,
    /// Always lowercase; the schema enforces it.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailbox {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub domain_id: Uuid,
    /// `local_part@domain`, always lowercase.
    pub address: String,
    /// Optional human label set from the profile page. Never used for
    /// authentication; the address remains the identity.
    pub display_name: Option<String>,
    pub quota_bytes: i64,
    pub used_bytes: i64,
}

impl Mailbox {
    pub fn free_bytes(&self) -> i64 {
        (self.quota_bytes - self.used_bytes).max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub id: Uuid,
    pub mailbox_id: Uuid,
    /// `Message-ID` without angle brackets (as `mail-parser` returns it).
    pub message_id_hdr: Option<String>,
    pub subject: Option<String>,
    pub from_addr: String,
    pub size_bytes: i64,
    /// Key into the [`crate::blob::BlobStore`].
    pub blob_key: String,
    pub received_at: DateTime<Utc>,
}

/// One durable outbound send intent waiting in (or moving through) the queue.
///
/// Mirrors the composed fields of the send form, persisted so a failed delivery
/// can be retried by the background worker instead of being dropped. Plain data
/// only — no behaviour, no SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundQueueEntry {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub mailbox_id: Uuid,
    pub from_addr: String,
    /// Comma-separated, lowercase recipient list (the validated compose shape).
    pub recipients: String,
    pub subject: String,
    pub body: String,
    /// Dispatches performed so far (0 = queued, first claim increments to 1).
    pub attempts: i32,
    pub max_attempts: i32,
    pub status: String,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Caller-supplied dedupe key for machine submissions. `None` for rows the
    /// browser compose form parked after a failed first attempt.
    pub idempotency_key: Option<String>,
    /// Value of the `X-Jami3ati-Notification` header: ties this mail to the
    /// notification row that produced it.
    pub correlation_id: Option<String>,
    /// Ready-to-send `List-Unsubscribe` value (already validated).
    pub list_unsubscribe: Option<String>,
    /// Optional `Reply-To` address (already validated).
    pub reply_to: Option<String>,
}
