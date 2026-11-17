//! Live, behavioural proof of the storage-layer guarantees.
//!
//! The HTTP proof endpoint must not repeat what a migration file *says*. It
//! asks the running database what it actually does:
//!
//! * open a transaction as `tira_app`, set the tenant GUC to a UUID that does
//!   not exist, and count mailboxes — the answer must be zero. If RLS were
//!   absent, this would return every mailbox in the database;
//! * read `rolbypassrls` for the effective role;
//! * ask whether `tira_app` holds any privilege on the two crown-jewel tables;
//! * attempt an audit append for a real tenant inside a transaction that is
//!   rolled back, so the proof verifies the write path without leaving a
//!   self-referential audit row behind.
//!
//! Everything here is read-only from the caller's point of view. The only
//! write is the audit probe, and it is rolled back before this function
//! returns.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::{PgStore, StoreError};

/// Which privileges `tira_app` holds on the two tables that must stay dark.
#[derive(Debug, Clone)]
pub struct CrownJewelProof {
    pub api_tokens_select: bool,
    pub api_tokens_insert: bool,
    pub message_recipients_select: bool,
    pub message_recipients_insert: bool,
}

impl CrownJewelProof {
    /// True only when all four probes came back false. A `true` here means the
    /// application role can touch a crown-jewel table, which is a regression
    /// the endpoint must surface, not hide.
    pub fn deny_all_verified(&self) -> bool {
        !self.api_tokens_select
            && !self.api_tokens_insert
            && !self.message_recipients_select
            && !self.message_recipients_insert
    }
}

/// What the audit trail looks like right now.
#[derive(Debug, Clone)]
pub struct AuditProof {
    /// Newest row, read as the owner role. `None` means the trail is empty.
    pub last_event_at: Option<DateTime<Utc>>,
    /// Real events written during the last seven days.
    pub events_last_7d: i64,
    /// `None` when the database has no tenant to probe with.
    pub insert_allowed: Option<bool>,
}

/// The result of one live probe against PostgreSQL.
#[derive(Debug, Clone)]
pub struct LiveProof {
    /// Effective role during the isolation probe. Must be `tira_app`.
    pub role: String,
    pub bypassrls: bool,
    /// Mailboxes visible with a random tenant GUC and no predicate.
    pub random_tenant_rows: i64,
    pub crown_jewels: CrownJewelProof,
    pub audit: AuditProof,
}

impl PgStore {
    /// Runs the probes described in the module docs.
    ///
    /// Errors are infrastructure errors (connection lost, role missing); a
    /// *failed* guarantee is not an error, it is data the endpoint reports.
    pub async fn live_proof(&self) -> Result<LiveProof, StoreError> {
        let mut client = self.client().await?;

        // ---- behavioural isolation probe (rolled back) --------------------
        //
        // A random tenant UUID is used on purpose: it cannot exist, so any
        // non-zero mailbox count can only come from RLS not filtering.
        let random_tenant = Uuid::new_v4();
        let tx = client.transaction().await?;
        tx.execute(
            "SELECT set_config('tira.tenant_id', $1, true)",
            &[&random_tenant.to_string()],
        )
        .await?;
        tx.batch_execute("SET LOCAL ROLE tira_app").await?;

        let row = tx
            .query_one(
                "SELECT current_user::text,
                        (SELECT rolbypassrls FROM pg_roles WHERE rolname = current_user),
                        (SELECT count(*) FROM mailboxes),
                        has_table_privilege(current_user, 'api_tokens', 'SELECT'),
                        has_table_privilege(current_user, 'api_tokens', 'INSERT'),
                        has_table_privilege(current_user, 'message_recipients', 'SELECT'),
                        has_table_privilege(current_user, 'message_recipients', 'INSERT')",
                &[],
            )
            .await?;

        let role: String = row.get(0);
        let bypassrls: bool = row.get(1);
        let random_tenant_rows: i64 = row.get(2);
        let crown_jewels = CrownJewelProof {
            api_tokens_select: row.get(3),
            api_tokens_insert: row.get(4),
            message_recipients_select: row.get(5),
            message_recipients_insert: row.get(6),
        };

        tx.rollback().await?;

        // ---- audit write-path probe (rolled back) -------------------------
        //
        // The row must never survive: an endpoint that audits itself turns
        // "the audit log is active" into a tautology. We pick a real tenant
        // because the policy's WITH CHECK would reject a random one.
        let tx = client.transaction().await?;
        let tenant: Option<Uuid> = tx
            .query_opt("SELECT id FROM tenants ORDER BY id LIMIT 1", &[])
            .await?
            .map(|row| row.get(0));

        let insert_allowed = match tenant {
            None => None,
            Some(tenant) => {
                tx.execute(
                    "SELECT set_config('tira.tenant_id', $1, true)",
                    &[&tenant.to_string()],
                )
                .await?;
                tx.batch_execute("SET LOCAL ROLE tira_app").await?;
                let allowed = tx
                    .execute(
                        "INSERT INTO audit_log (tenant_id, actor, action, result)
                         VALUES ($1, 'proof:probe', 'proof.probe', 'ok')",
                        &[&tenant],
                    )
                    .await
                    .is_ok();
                Some(allowed)
            }
        };
        tx.rollback().await?;

        // ---- observed activity, read as the owner -------------------------
        let stats = client
            .query_one(
                "SELECT max(at),
                        count(*) FILTER (WHERE at > now() - interval '7 days')
                   FROM audit_log",
                &[],
            )
            .await?;

        Ok(LiveProof {
            role,
            bypassrls,
            random_tenant_rows,
            crown_jewels,
            audit: AuditProof {
                last_event_at: stats.get(0),
                events_last_7d: stats.get(1),
                insert_allowed,
            },
        })
    }
}
