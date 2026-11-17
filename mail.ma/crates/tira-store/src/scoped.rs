//! Tenant-scoped reads under row-level security.
//!
//! Every function here runs inside a transaction that has done two things:
//!
//! ```sql
//! SELECT set_config('tira.tenant_id', '<tenant>', true);
//! SET LOCAL ROLE tira_app;
//! ```
//!
//! `tira_app` has no `BYPASSRLS`, and `migration 0004` put a policy on every
//! tenant table, so the database — not the SQL in this file — decides which
//! rows exist. The explicit `tenant_id = $2` predicates are kept as a second
//! line of defence: if a deployment ever runs the read path as the owner by
//! mistake, the query is still scoped. The policies are proven separately in
//! `tests/rls.rs`, with no predicate to lean on.
//!
//! Two consequences worth stating:
//!
//! - **A read that finds nothing is still a write.** A cross-tenant attempt
//!   appends `read.denied` for the *caller's* tenant. The response stays 404,
//!   so ids cannot be probed, but compliance can count the attempts.
//! - **The audit row commits with the read.** If the audit insert fails, the
//!   request fails. A mailbox that can be read without a trace is worse than
//!   an unavailable mailbox.

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use deadpool_postgres::Transaction;
use uuid::Uuid;

use crate::model::{Mailbox, StoredMessage};
use crate::store::{PgStore, StoreError};

/// Who is reading, for the audit trail.
///
/// `actor` is `token:<uuid>`, never the token secret: the trail must be
/// useful without becoming a second place a credential can leak from.
#[derive(Debug, Clone)]
pub struct AuditContext {
    pub request_id: Uuid,
    pub actor: String,
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

/// One page of a mailbox, resolved in a single tenant-scoped transaction.
#[derive(Debug, Clone)]
pub struct MailboxPage {
    pub mailbox: Mailbox,
    pub total: i64,
    pub messages: Vec<StoredMessage>,
}

/// A row from `audit_log`, for CNDP subject-access requests and operators.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: i64,
    pub tenant_id: Option<Uuid>,
    pub actor: String,
    pub action: String,
    pub object: Option<String>,
    pub at: DateTime<Utc>,
    pub request_id: Option<Uuid>,
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub result: String,
    /// Raw JSON text, so the store does not force a JSON type on its callers.
    pub metadata: String,
}

/// Opens a transaction and drops it to the unprivileged application role.
///
/// The GUC is set with `is_local = true` *and* the role with `SET LOCAL`, so
/// both disappear at commit or rollback. A pooled connection cannot carry a
/// tenant from one request into the next.
async fn tenant_tx<'a>(
    client: &'a mut deadpool_postgres::Object,
    tenant_id: Uuid,
) -> Result<Transaction<'a>, StoreError> {
    let tx = client.transaction().await?;
    tx.execute(
        "SELECT set_config('tira.tenant_id', $1, true)",
        &[&tenant_id.to_string()],
    )
    .await?;
    tx.batch_execute("SET LOCAL ROLE tira_app").await?;
    Ok(tx)
}

async fn record(
    tx: &Transaction<'_>,
    tenant_id: Uuid,
    audit: &AuditContext,
    action: &str,
    object: Option<&str>,
    result: &str,
    metadata: serde_json::Value,
) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO audit_log
             (tenant_id, actor, action, object, result, request_id, ip, user_agent, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb)",
        &[
            &tenant_id,
            &audit.actor,
            &action,
            &object,
            &result,
            &audit.request_id,
            &audit.ip,
            &audit.user_agent,
            &metadata,
        ],
    )
    .await?;
    Ok(())
}

impl PgStore {
    /// Lists one page of a mailbox. `None` means "not visible to this caller":
    /// wrong tenant, wrong mailbox scope, or genuinely absent. The caller must
    /// turn all three into 404 — never 403 — so ids cannot be probed.
    ///
    /// `mailbox_scope` is the token's optional restriction: `None` = any
    /// mailbox in the tenant, `Some(id)` = that mailbox only.
    pub async fn read_mailbox_page(
        &self,
        mailbox_id: Uuid,
        tenant_id: Uuid,
        mailbox_scope: Option<Uuid>,
        limit: i64,
        offset: i64,
        audit: &AuditContext,
    ) -> Result<Option<MailboxPage>, StoreError> {
        let mut client = self.client().await?;
        let tx = tenant_tx(&mut client, tenant_id).await?;

        let mailbox = tx
            .query_opt(
                "SELECT m.id, m.tenant_id, m.domain_id,
                        m.local_part || '@' || d.name AS address,
                        m.display_name, m.quota_bytes, m.used_bytes
                   FROM mailboxes m
                   JOIN domains d ON d.id = m.domain_id
                  WHERE m.id = $1
                    AND m.tenant_id = $2
                    AND ($3::uuid IS NULL OR m.id = $3)",
                &[&mailbox_id, &tenant_id, &mailbox_scope],
            )
            .await?
            .map(|row| Mailbox {
                id: row.get(0),
                tenant_id: row.get(1),
                domain_id: row.get(2),
                address: row.get(3),
                display_name: row.get(4),
                quota_bytes: row.get(5),
                used_bytes: row.get(6),
            });

        let Some(mailbox) = mailbox else {
            record(
                &tx,
                tenant_id,
                audit,
                "read.denied",
                Some(&mailbox_id.to_string()),
                "denied",
                serde_json::json!({
                    "resource": "mailbox",
                    "reason": "not_visible",
                    "limit": limit,
                    "offset": offset,
                }),
            )
            .await?;
            tx.commit().await?;
            return Ok(None);
        };

        let total: i64 = tx
            .query_one(
                "SELECT count(*) FROM messages WHERE mailbox_id = $1",
                &[&mailbox_id],
            )
            .await?
            .get(0);

        let rows = tx
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

        let messages: Vec<StoredMessage> = rows
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
            .collect();

        record(
            &tx,
            tenant_id,
            audit,
            "mailbox.read",
            Some(&mailbox_id.to_string()),
            "ok",
            serde_json::json!({
                "limit": limit,
                "offset": offset,
                "returned": messages.len(),
                "total": total,
            }),
        )
        .await?;

        tx.commit().await?;

        Ok(Some(MailboxPage {
            mailbox,
            total,
            messages,
        }))
    }

    /// Returns the exact bytes accepted over SMTP, after proving the caller may
    /// see the message. The blob is read *inside* the transaction so the audit
    /// row and the returned bytes cannot disagree about whether a read
    /// happened. The disk read itself runs on the blocking pool
    /// ([`PgStore::raw_message_async`]); the transaction stays open across that
    /// await, so the audit/commit ordering is unchanged while the runtime's
    /// worker threads are never parked on `std::fs::read`.
    pub async fn read_message_bytes(
        &self,
        message_id: Uuid,
        tenant_id: Uuid,
        mailbox_scope: Option<Uuid>,
        audit: &AuditContext,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let mut client = self.client().await?;
        let tx = tenant_tx(&mut client, tenant_id).await?;

        let message = tx
            .query_opt(
                "SELECT msg.id, msg.mailbox_id, msg.message_id_hdr, msg.subject,
                        msg.from_addr, msg.size_bytes, msg.blob_key, msg.received_at
                   FROM messages msg
                   JOIN mailboxes mb ON mb.id = msg.mailbox_id
                  WHERE msg.id = $1
                    AND mb.tenant_id = $2
                    AND ($3::uuid IS NULL OR msg.mailbox_id = $3)",
                &[&message_id, &tenant_id, &mailbox_scope],
            )
            .await?
            .map(|row| StoredMessage {
                id: row.get(0),
                mailbox_id: row.get(1),
                message_id_hdr: row.get(2),
                subject: row.get(3),
                from_addr: row.get(4),
                size_bytes: row.get(5),
                blob_key: row.get(6),
                received_at: row.get::<_, DateTime<Utc>>(7),
            });

        let Some(message) = message else {
            record(
                &tx,
                tenant_id,
                audit,
                "read.denied",
                Some(&message_id.to_string()),
                "denied",
                serde_json::json!({
                    "resource": "message",
                    "reason": "not_visible",
                }),
            )
            .await?;
            tx.commit().await?;
            return Ok(None);
        };

        let bytes = self.raw_message_async(&message.blob_key).await?;

        record(
            &tx,
            tenant_id,
            audit,
            "message.read",
            Some(&message.id.to_string()),
            "ok",
            serde_json::json!({
                "mailbox_id": message.mailbox_id,
                "size_bytes": message.size_bytes,
            }),
        )
        .await?;

        tx.commit().await?;
        Ok(Some(bytes))
    }

    /// Reads a tenant's access trail, newest first.
    ///
    /// This is the **owner** path, not the application role: it is meant for
    /// answering a CNDP subject-access request or for an operator console, and
    /// it must not be reachable from the public read API.
    pub async fn recent_audit(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        let client = self.client().await?;
        let rows = client
            .query(
                "SELECT id, tenant_id, actor, action, object, at,
                        request_id, ip, user_agent, result, metadata::text
                   FROM audit_log
                  WHERE tenant_id = $1
                  ORDER BY at DESC, id DESC
                  LIMIT $2",
                &[&tenant_id, &limit],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|row| AuditEntry {
                id: row.get(0),
                tenant_id: row.get(1),
                actor: row.get(2),
                action: row.get(3),
                object: row.get(4),
                at: row.get(5),
                request_id: row.get(6),
                ip: row.get(7),
                user_agent: row.get(8),
                result: row.get(9),
                metadata: row.get(10),
            })
            .collect())
    }
}
