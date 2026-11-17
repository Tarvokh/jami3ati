//! PostgreSQL-backed metadata store.
//!
//! This is the missing piece the SMTP server has been writing into a void:
//! a real [`MailHandler`] that resolves recipients, enforces quota, writes the
//! blob, and commits metadata — or fails the whole delivery.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use uuid::Uuid;

use tira_smtp::message::ParsedMessage;
use tira_smtp::server::{DeliveryError, Envelope, MailHandler};

use crate::blob::{BlobError, BlobStore, FsBlobStore};
use crate::db;
use crate::model::{Domain, Mailbox, OutboundQueueEntry, StoredMessage, Tenant};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] tokio_postgres::Error),
    #[error("connection pool: {0}")]
    Pool(String),
    #[error("invalid database url: {0}")]
    Config(String),
    #[error("blob error: {0}")]
    Blob(#[from] BlobError),
    #[error("blocking read task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("no such recipient: {0}")]
    UnknownRecipient(String),
    #[error("quota exceeded for {0}")]
    QuotaExceeded(String),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid engine state: {0}")]
    InvalidState(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid token: {0}")]
    InvalidToken(String),
}

/// Cheap to clone: the pool and the blob store are both behind `Arc`.
#[derive(Clone)]
pub struct PgStore {
    pool: Pool,
    blobs: Arc<dyn BlobStore>,
}

impl std::fmt::Debug for PgStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgStore").finish_non_exhaustive()
    }
}

impl PgStore {
    /// Connects, runs migrations, and returns a ready store.
    pub async fn connect(
        database_url: &str,
        blob_root: impl Into<PathBuf>,
    ) -> Result<Self, StoreError> {
        let pool = db::build_pool(database_url, 16)?;
        let blobs = Arc::new(FsBlobStore::new(blob_root)?);
        let store = Self { pool, blobs };
        db::migrate(&store.pool).await?;
        Ok(store)
    }

    pub(crate) async fn client(&self) -> Result<deadpool_postgres::Object, StoreError> {
        self.pool
            .get()
            .await
            .map_err(|err| StoreError::Pool(err.to_string()))
    }

    /// Round-trips a trivial query. Used by `/healthz` so a process that is
    /// alive but has lost its database reports itself as unhealthy instead of
    /// accepting mail it cannot store.
    pub async fn ping(&self) -> Result<(), StoreError> {
        let client = self.client().await?;
        client.query_one("SELECT 1", &[]).await?;
        Ok(())
    }

    // ---- tenancy -------------------------------------------------------

    pub async fn create_tenant(
        &self,
        name: &str,
        cndp_ref: Option<&str>,
    ) -> Result<Tenant, StoreError> {
        let id = Uuid::new_v4();
        let client = self.client().await?;
        client
            .execute(
                "INSERT INTO tenants (id, name, cndp_ref) VALUES ($1, $2, $3)",
                &[&id, &name, &cndp_ref],
            )
            .await?;
        Ok(Tenant {
            id,
            name: name.to_string(),
            cndp_ref: cndp_ref.map(str::to_string),
        })
    }

    pub async fn create_domain(&self, tenant_id: Uuid, name: &str) -> Result<Domain, StoreError> {
        let id = Uuid::new_v4();
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || !name.contains('.') {
            return Err(StoreError::InvalidAddress(name));
        }
        let client = self.client().await?;
        client
            .execute(
                "INSERT INTO domains (id, tenant_id, name) VALUES ($1, $2, $3)",
                &[&id, &tenant_id, &name],
            )
            .await?;
        Ok(Domain {
            id,
            tenant_id,
            name,
        })
    }

    pub async fn create_mailbox(
        &self,
        domain_id: Uuid,
        local_part: &str,
        quota_bytes: i64,
    ) -> Result<Mailbox, StoreError> {
        let local_part = local_part.trim().to_ascii_lowercase();
        if local_part.is_empty() || local_part.contains('@') {
            return Err(StoreError::InvalidAddress(local_part));
        }

        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT tenant_id, name FROM domains WHERE id = $1",
                &[&domain_id],
            )
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("domain {domain_id}")))?;

        let tenant_id: Uuid = row.get(0);
        let domain_name: String = row.get(1);
        let id = Uuid::new_v4();

        client
            .execute(
                "INSERT INTO mailboxes (id, tenant_id, domain_id, local_part, quota_bytes)
                 VALUES ($1, $2, $3, $4, $5)",
                &[&id, &tenant_id, &domain_id, &local_part, &quota_bytes],
            )
            .await?;

        Ok(Mailbox {
            id,
            tenant_id,
            domain_id,
            address: format!("{local_part}@{domain_name}"),
            display_name: None,
            quota_bytes,
            used_bytes: 0,
        })
    }

    // ---- delivery ------------------------------------------------------

    /// Finds the mailbox that owns `address`, or `None`.
    ///
    /// This runs as the owner role through `tira_resolve_recipient`, because
    /// SMTP asks the question *before* a tenant exists. `tira_app` cannot call
    /// it: the function is the documented escape hatch around RLS, not a
    /// general-purpose lookup.
    pub async fn resolve_recipient(&self, address: &str) -> Result<Option<Mailbox>, StoreError> {
        let address = address.trim().to_ascii_lowercase();
        let (local, domain) = match address.split_once('@') {
            Some(parts) => parts,
            None => return Ok(None),
        };
        if local.is_empty() || domain.is_empty() {
            return Ok(None);
        }

        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT mailbox_id, mailbox_tenant_id, mailbox_domain_id,
                        address, quota_bytes, used_bytes
                   FROM tira_resolve_recipient($1, $2)",
                &[&local, &domain],
            )
            .await?;

        Ok(row.map(|row| Mailbox {
            id: row.get(0),
            tenant_id: row.get(1),
            domain_id: row.get(2),
            address: row.get(3),
            // `tira_resolve_recipient` predates the profile label and returns
            // only delivery facts. The label is never needed at RCPT time.
            display_name: None,
            quota_bytes: row.get(4),
            used_bytes: row.get(5),
        }))
    }

    /// Looks up a mailbox by primary key. The HTTP read path needs this to
    /// distinguish "empty mailbox" from "no such mailbox".
    pub async fn get_mailbox(&self, id: Uuid) -> Result<Option<Mailbox>, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT m.id, m.tenant_id, m.domain_id,
                        m.local_part || '@' || d.name AS address,
                        m.display_name, m.quota_bytes, m.used_bytes
                   FROM mailboxes m
                   JOIN domains d ON d.id = m.domain_id
                  WHERE m.id = $1",
                &[&id],
            )
            .await?;

        Ok(row.map(|row| Mailbox {
            id: row.get(0),
            tenant_id: row.get(1),
            domain_id: row.get(2),
            address: row.get(3),
            display_name: row.get(4),
            quota_bytes: row.get(5),
            used_bytes: row.get(6),
        }))
    }

    /// Stores one message for every envelope recipient.
    ///
    /// The blob is written first (idempotent, content-addressed) and the
    /// metadata is committed in a single transaction. If any recipient is
    /// unknown or over quota, nothing is committed.
    pub async fn append_message(
        &self,
        envelope: &Envelope,
        message: &ParsedMessage,
        raw: &[u8],
    ) -> Result<Vec<StoredMessage>, StoreError> {
        if envelope.rcpt_to.is_empty() {
            return Err(StoreError::InvalidAddress("no recipients".to_string()));
        }
        if raw.is_empty() {
            return Err(StoreError::InvalidAddress("empty message".to_string()));
        }

        let mut targets = Vec::with_capacity(envelope.rcpt_to.len());
        for rcpt in &envelope.rcpt_to {
            match self.resolve_recipient(rcpt).await? {
                Some(mailbox) => targets.push(mailbox),
                None => return Err(StoreError::UnknownRecipient(rcpt.clone())),
            }
        }

        // Take the per-mailbox `FOR UPDATE` locks below in one global order
        // (ascending mailbox id). Two concurrent fan-out deliveries that share
        // mailboxes but list them in opposite envelope order would otherwise
        // lock A→B and B→A and deadlock; PostgreSQL aborts one transaction with
        // a deadlock error, which surfaces as a retryable 451 and bounces mail
        // that should have been accepted. Sorting makes every delivery acquire
        // the same rows in the same order, so the second transaction simply
        // waits for the first instead of forming a cycle. The returned copies
        // are therefore ordered by mailbox id, not by envelope position; no
        // caller relies on the latter (the SMTP handler discards the vector).
        targets.sort_unstable_by_key(|mailbox| mailbox.id);

        let size = raw.len() as i64;

        // Pre-flight quota: refuse *before* writing any bytes. Without this a
        // sender could exhaust the disk by repeatedly delivering to a full
        // mailbox — the blob was written, the transaction rolled back, and an
        // orphan was left behind every time. The authoritative check still
        // runs under `FOR UPDATE` below (the resolved figures can be stale
        // under concurrency); this cheap guard stops the repeatable attack at
        // the source, and the orphan sweep collects any residual race.
        for mailbox in &targets {
            if mailbox.used_bytes + size > mailbox.quota_bytes {
                return Err(StoreError::QuotaExceeded(mailbox.address.clone()));
            }
        }

        // One envelope can fan out to mailboxes in several tenants. Collect
        // the distinct tenants so each gets its own audit row below.
        let mut tenant_ids: Vec<Uuid> = targets.iter().map(|m| m.tenant_id).collect();
        tenant_ids.sort_unstable();
        tenant_ids.dedup();

        let blob = self.blobs.put(raw)?;
        let now = Utc::now();

        let mut client = self.client().await?;
        let tx = client.transaction().await?;

        let mut stored = Vec::with_capacity(targets.len());
        for mailbox in targets {
            // Lock the mailbox row so two concurrent deliveries cannot both
            // pass the quota check and overfill the account.
            let locked = tx
                .query_one(
                    "SELECT used_bytes, quota_bytes FROM mailboxes WHERE id = $1 FOR UPDATE",
                    &[&mailbox.id],
                )
                .await?;
            let used: i64 = locked.get(0);
            let quota: i64 = locked.get(1);

            if used + size > quota {
                tx.rollback().await?;
                return Err(StoreError::QuotaExceeded(mailbox.address));
            }

            let id = Uuid::new_v4();
            tx.execute(
                "INSERT INTO messages
                     (id, tenant_id, mailbox_id, message_id_hdr, subject, from_addr,
                      size_bytes, blob_key, sha256, received_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &id,
                    &mailbox.tenant_id,
                    &mailbox.id,
                    &message.message_id,
                    &message.subject,
                    &envelope.mail_from,
                    &size,
                    &blob.key,
                    &blob.sha256_hex,
                    &now,
                ],
            )
            .await?;

            // One row per copy, recording the address this copy belongs to.
            // Storing the whole envelope here would make mailbox A's message
            // claim it was delivered to mailbox B.
            tx.execute(
                "INSERT INTO message_recipients (message_id, address)
                 VALUES ($1, $2) ON CONFLICT DO NOTHING",
                &[&id, &mailbox.address],
            )
            .await?;

            tx.execute(
                "UPDATE mailboxes SET used_bytes = used_bytes + $1 WHERE id = $2",
                &[&size, &mailbox.id],
            )
            .await?;

            stored.push(StoredMessage {
                id,
                mailbox_id: mailbox.id,
                message_id_hdr: message.message_id.clone(),
                subject: message.subject.clone(),
                from_addr: envelope.mail_from.clone(),
                size_bytes: size,
                blob_key: blob.key.clone(),
                received_at: now,
            });
        }

        // One audit row per tenant that received a copy. Recording only the
        // first recipient's tenant would leave every other tenant's delivery
        // untraceable, which CNDP 09-08 does not allow. The rows share the
        // envelope sender and the blob key, but each is scoped to its tenant.
        for tenant_id in &tenant_ids {
            tx.execute(
                "INSERT INTO audit_log (tenant_id, actor, action, object)
                 VALUES ($1, $2, $3, $4)",
                &[
                    tenant_id,
                    &envelope.mail_from,
                    &"message.delivered",
                    &blob.key,
                ],
            )
            .await?;
        }

        tx.commit().await?;
        Ok(stored)
    }

    // ---- reading -------------------------------------------------------

    pub async fn list_mailbox(
        &self,
        mailbox_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        let client = self.client().await?;
        let rows = client
            .query(
                "SELECT id, mailbox_id, message_id_hdr, subject, from_addr,
                        size_bytes, blob_key, received_at
                   FROM messages
                  WHERE mailbox_id = $1
                  ORDER BY received_at DESC, id
                  LIMIT $2 OFFSET $3",
                &[&mailbox_id, &limit, &offset],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|row| StoredMessage {
                id: row.get(0),
                mailbox_id: row.get(1),
                message_id_hdr: row.get(2),
                subject: row.get(3),
                from_addr: row.get(4),
                size_bytes: row.get(5),
                blob_key: row.get(6),
                received_at: row.get::<_, DateTime<Utc>>(7),
            })
            .collect())
    }

    /// Looks up one message by its primary key. Used by the HTTP read path:
    /// a client has the id from a listing, never the blob key, so the blob key
    /// must not be the thing an unauthenticated caller can guess.
    pub async fn get_message(&self, id: Uuid) -> Result<Option<StoredMessage>, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT id, mailbox_id, message_id_hdr, subject, from_addr,
                        size_bytes, blob_key, received_at
                   FROM messages
                  WHERE id = $1",
                &[&id],
            )
            .await?;

        Ok(row.map(|row| StoredMessage {
            id: row.get(0),
            mailbox_id: row.get(1),
            message_id_hdr: row.get(2),
            subject: row.get(3),
            from_addr: row.get(4),
            size_bytes: row.get(5),
            blob_key: row.get(6),
            received_at: row.get::<_, DateTime<Utc>>(7),
        }))
    }

    /// Tenant-scoped message lookup. Same contract as
    /// [`Self::get_mailbox_scoped`]: `None` means "not yours or not there",
    /// and the caller must not distinguish the two.
    pub async fn get_message_scoped(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        mailbox_scope: Option<Uuid>,
    ) -> Result<Option<StoredMessage>, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT msg.id, msg.mailbox_id, msg.message_id_hdr, msg.subject,
                        msg.from_addr, msg.size_bytes, msg.blob_key, msg.received_at
                   FROM messages msg
                   JOIN mailboxes mb ON mb.id = msg.mailbox_id
                  WHERE msg.id = $1
                    AND mb.tenant_id = $2
                    AND ($3::uuid IS NULL OR msg.mailbox_id = $3)",
                &[&id, &tenant_id, &mailbox_scope],
            )
            .await?;

        Ok(row.map(|row| StoredMessage {
            id: row.get(0),
            mailbox_id: row.get(1),
            message_id_hdr: row.get(2),
            subject: row.get(3),
            from_addr: row.get(4),
            size_bytes: row.get(5),
            blob_key: row.get(6),
            received_at: row.get::<_, DateTime<Utc>>(7),
        }))
    }

    /// Returns the exact bytes that were accepted over SMTP, hash-verified.
    ///
    /// Synchronous form for callers already on a blocking thread (and for
    /// tests). Async request handlers must use [`PgStore::raw_message_async`]
    /// so a slow disk read never parks a runtime worker thread.
    pub fn raw_message(&self, blob_key: &str) -> Result<Vec<u8>, StoreError> {
        Ok(self.blobs.get(blob_key)?)
    }

    /// Async form of [`PgStore::raw_message`]: the content-addressed blob is
    /// read and hash-verified on the blocking pool (`std::fs::read`), keeping
    /// the runtime's worker threads free for other requests. The blob handle
    /// and key are cloned into the closure, so nothing borrows `self` across
    /// the await and any surrounding transaction is left untouched. A
    /// [`StoreError::Task`] means the blocking task itself panicked or was
    /// cancelled — distinct from a corrupt blob ([`StoreError::Blob`]).
    pub async fn raw_message_async(&self, blob_key: &str) -> Result<Vec<u8>, StoreError> {
        let blobs = Arc::clone(&self.blobs);
        let key = blob_key.to_string();
        let bytes = tokio::task::spawn_blocking(move || blobs.get(&key))
            .await
            .map_err(StoreError::Task)??;
        Ok(bytes)
    }

    pub async fn count_messages(&self, mailbox_id: Uuid) -> Result<i64, StoreError> {
        let client = self.client().await?;
        let row = client
            .query_one(
                "SELECT count(*) FROM messages WHERE mailbox_id = $1",
                &[&mailbox_id],
            )
            .await?;
        Ok(row.get(0))
    }

    // ---- outbound queue ------------------------------------------------

    /// Durably records a composed send that just failed its delivery attempt.
    ///
    /// The row enters `pending` with `attempts = 0`; the background worker
    /// claims it once `next_attempt_at` passes and replays the send. Nothing is
    /// dispatched here — the caller has already spoken to the relay and got a
    /// failure, and the honest error is still returned to the human.
    pub async fn enqueue_outbound(
        &self,
        tenant_id: Uuid,
        mailbox_id: Uuid,
        from_addr: &str,
        recipients: &str,
        subject: &str,
        body: &str,
    ) -> Result<Uuid, StoreError> {
        let id = Uuid::new_v4();
        let client = self.client().await?;
        client
            .execute(
                "INSERT INTO outbound_queue
                     (id, tenant_id, mailbox_id, from_addr, recipients, subject, body,
                      attempts, status, next_attempt_at, last_error)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 0, 'pending', now(), NULL)",
                &[
                    &id,
                    &tenant_id,
                    &mailbox_id,
                    &from_addr,
                    &recipients,
                    &subject,
                    &body,
                ],
            )
            .await?;
        Ok(id)
    }

    /// Accepts a machine-submitted send into the durable queue.
    ///
    /// Unlike [`PgStore::enqueue_outbound`], this is the *first* write, not a
    /// post-failure fallback: the caller gets a durable acceptance and the
    /// background worker performs the delivery. Returns `(id, created)`;
    /// `created == false` means `(tenant_id, idempotency_key)` already existed,
    /// so the caller's retry is a no-op that resolves to the same queue row.
    ///
    /// The `ON CONFLICT` target names the partial unique index predicate
    /// (`WHERE idempotency_key IS NOT NULL`), which PostgreSQL requires for
    /// inference against a partial index.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_outbound_api(
        &self,
        tenant_id: Uuid,
        mailbox_id: Uuid,
        from_addr: &str,
        recipients: &str,
        subject: &str,
        body: &str,
        idempotency_key: &str,
        correlation_id: Option<&str>,
        list_unsubscribe: Option<&str>,
        reply_to: Option<&str>,
    ) -> Result<(Uuid, bool), StoreError> {
        let id = Uuid::new_v4();
        let client = self.client().await?;
        let inserted = client
            .query_opt(
                "INSERT INTO outbound_queue
                     (id, tenant_id, mailbox_id, from_addr, recipients, subject, body,
                      attempts, status, next_attempt_at, last_error,
                      idempotency_key, correlation_id, list_unsubscribe, reply_to)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 0, 'pending', now(), NULL,
                         $8, $9, $10, $11)
                 ON CONFLICT (tenant_id, idempotency_key)
                     WHERE idempotency_key IS NOT NULL
                     DO NOTHING
                 RETURNING id",
                &[
                    &id,
                    &tenant_id,
                    &mailbox_id,
                    &from_addr,
                    &recipients,
                    &subject,
                    &body,
                    &idempotency_key,
                    &correlation_id,
                    &list_unsubscribe,
                    &reply_to,
                ],
            )
            .await?;

        if let Some(row) = inserted {
            return Ok((row.get(0), true));
        }

        // The key already exists: return the row the first caller created so a
        // retried request is observably idempotent rather than a silent drop.
        let row = client
            .query_one(
                "SELECT id FROM outbound_queue
                  WHERE tenant_id = $1 AND idempotency_key = $2",
                &[&tenant_id, &idempotency_key],
            )
            .await?;
        Ok((row.get(0), false))
    }

    /// Claims up to `limit` due outbound sends, oldest first, using
    /// `FOR UPDATE SKIP LOCKED` so a second worker never takes the same row.
    /// Each claim increments `attempts`; the worker then either completes the
    /// row, reschedules it with backoff, or dead-letters it.
    pub async fn claim_due_outbound(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboundQueueEntry>, StoreError> {
        let client = self.client().await?;
        let limit_i64 = limit as i64;
        let rows = client
            .query(
                "WITH picked AS (
                     SELECT id FROM outbound_queue
                      WHERE status = 'pending'
                        AND next_attempt_at <= now()
                      ORDER BY created_at ASC
                      FOR UPDATE SKIP LOCKED
                      LIMIT $1
                 )
                 UPDATE outbound_queue AS q
                    SET attempts = q.attempts + 1,
                        updated_at = now()
                   FROM picked
                  WHERE q.id = picked.id
             RETURNING q.id, q.tenant_id, q.mailbox_id, q.from_addr, q.recipients,
                       q.subject, q.body, q.attempts, q.max_attempts, q.status,
                       q.next_attempt_at, q.last_error, q.created_at, q.updated_at,
                       q.idempotency_key, q.correlation_id, q.list_unsubscribe,
                       q.reply_to",
                &[&limit_i64],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|row| OutboundQueueEntry {
                id: row.get(0),
                tenant_id: row.get(1),
                mailbox_id: row.get(2),
                from_addr: row.get(3),
                recipients: row.get(4),
                subject: row.get(5),
                body: row.get(6),
                attempts: row.get(7),
                max_attempts: row.get(8),
                status: row.get(9),
                next_attempt_at: row.get::<_, DateTime<Utc>>(10),
                last_error: row.get(11),
                created_at: row.get::<_, DateTime<Utc>>(12),
                updated_at: row.get::<_, DateTime<Utc>>(13),
                idempotency_key: row.get(14),
                correlation_id: row.get(15),
                list_unsubscribe: row.get(16),
                reply_to: row.get(17),
            })
            .collect())
    }

    /// Marks an outbound send as delivered (the delivery handler persisted it).
    pub async fn complete_outbound(&self, id: Uuid) -> Result<(), StoreError> {
        let client = self.client().await?;
        client
            .execute(
                "UPDATE outbound_queue SET status = 'sent', updated_at = now() WHERE id = $1",
                &[&id],
            )
            .await?;
        Ok(())
    }

    /// Reschedules a failed attempt for a later dispatch with backoff.
    pub async fn retry_outbound(
        &self,
        id: Uuid,
        error: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let client = self.client().await?;
        client
            .execute(
                "UPDATE outbound_queue
                    SET status = 'pending', next_attempt_at = $2, last_error = $3, updated_at = now()
                  WHERE id = $1",
                &[&id, &next_attempt_at, &error],
            )
            .await?;
        Ok(())
    }

    /// Dead-letters a send whose attempts are exhausted. The row is kept for
    /// the audit trail rather than deleted, so the failure is not silent.
    pub async fn dead_letter_outbound(&self, id: Uuid, error: &str) -> Result<(), StoreError> {
        let client = self.client().await?;
        client
            .execute(
                "UPDATE outbound_queue SET status = 'failed', last_error = $1, updated_at = now() WHERE id = $2",
                &[&error, &id],
            )
            .await?;
        Ok(())
    }

    /// Records a durable bounce: a send that exhausted its retry budget and
    /// permanently failed. `permanent` mirrors the dead-letter decision (a
    /// non-permanent failure would have been rescheduled instead).
    pub async fn record_bounce(
        &self,
        tenant_id: Uuid,
        mailbox_id: Uuid,
        recipient: &str,
        reason: &str,
        permanent: bool,
    ) -> Result<Uuid, StoreError> {
        let id = Uuid::new_v4();
        let client = self.client().await?;
        client
            .execute(
                "INSERT INTO bounces (id, tenant_id, mailbox_id, recipient, reason, permanent)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &id,
                    &tenant_id,
                    &mailbox_id,
                    &recipient,
                    &reason,
                    &permanent,
                ],
            )
            .await?;
        Ok(id)
    }

    /// Mark-and-sweep garbage collection for orphaned blobs.
    ///
    /// A blob can end up on disk with no `messages` row pointing at it: the
    /// delivery wrote the bytes, then the transaction rolled back (quota race,
    /// database error, crash). Content-addressed dedup makes *immediate*
    /// deletion on rollback unsafe — another delivery may already be
    /// referencing the same bytes — so cleanup is deferred to this sweep.
    ///
    /// **Mark:** every `blob_key` still referenced by a committed message.
    /// **Sweep:** every blob on disk whose mtime predates `now - grace` and
    /// whose key is not in the mark set. The grace window is the safety
    /// margin: `put` refreshes a blob's mtime on every delivery (even a
    /// deduplicated one), so any blob an open transaction might still commit
    /// against has a recent mtime and is never a candidate. `grace` must
    /// exceed the longest possible delivery transaction; the caller uses 24h.
    ///
    /// Returns the number of blobs removed. Referenced blobs are never
    /// touched, so this cannot erase a live message's bytes.
    pub async fn purge_unreferenced_blobs(
        &self,
        grace: chrono::Duration,
    ) -> Result<u64, StoreError> {
        use std::collections::HashSet;
        use std::time::SystemTime;

        let grace_std = grace.to_std().unwrap_or(std::time::Duration::ZERO);
        let cutoff = SystemTime::now()
            .checked_sub(grace_std)
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let client = self.client().await?;
        let rows = client
            .query("SELECT DISTINCT blob_key FROM messages", &[])
            .await?;
        let referenced: HashSet<String> = rows.iter().map(|row| row.get::<_, String>(0)).collect();
        drop(client);

        let candidates = self.blobs.keys_modified_before(cutoff)?;
        let mut removed = 0u64;
        for key in candidates {
            if referenced.contains(&key) {
                continue;
            }
            // Re-check mtime immediately before unlinking: a delivery that
            // deduplicated onto this blob after the candidate list was built
            // has refreshed its mtime, and this delete will now decline.
            if self.blobs.remove_if_modified_before(&key, cutoff)? {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[async_trait]
impl MailHandler for PgStore {
    async fn accepts_recipient(&self, address: &str) -> bool {
        match self.resolve_recipient(address).await {
            Ok(Some(_)) => true,
            Ok(None) => false,
            // Fail *open* at RCPT on a database error, so the message is
            // rejected later with a retryable 451 instead of a permanent 550
            // that would bounce the sender's mail.
            Err(err) => {
                tracing::warn!(%err, "recipient lookup failed; deferring decision");
                true
            }
        }
    }

    async fn deliver(
        &self,
        envelope: Envelope,
        message: ParsedMessage,
        raw: Vec<u8>,
    ) -> Result<(), DeliveryError> {
        match self.append_message(&envelope, &message, &raw).await {
            Ok(_) => Ok(()),
            Err(StoreError::UnknownRecipient(address)) => {
                Err(DeliveryError::UnknownRecipient(address))
            }
            Err(StoreError::QuotaExceeded(address)) => Err(DeliveryError::OverQuota(address)),
            Err(err) => Err(DeliveryError::Temporary(err.to_string())),
        }
    }
}
