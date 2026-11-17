//! Connection pool and migration runner.
//!
//! Migrations are embedded at compile time (`include_str!`) so the binary is
//! self-contained: no external `sqlx-cli`, no files to ship, no chance of
//! running a binary against a half-applied schema directory.
//!
//! The runner takes a transaction-scoped advisory lock, so two API instances
//! starting at the same time cannot both apply the same migration.

use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

use crate::store::StoreError;

/// Ordered migrations. Never edit an applied migration; add a new one.
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    ("0002_engine", include_str!("../migrations/0002_engine.sql")),
    ("0003_tokens", include_str!("../migrations/0003_tokens.sql")),
    ("0004_rls", include_str!("../migrations/0004_rls.sql")),
    ("0005_audit", include_str!("../migrations/0005_audit.sql")),
    (
        "0006_identity",
        include_str!("../migrations/0006_identity.sql"),
    ),
    (
        "0007_column_grants",
        include_str!("../migrations/0007_column_grants.sql"),
    ),
    (
        "0008_outbound_queue",
        include_str!("../migrations/0008_outbound_queue.sql"),
    ),
    (
        "0009_bounces",
        include_str!("../migrations/0009_bounces.sql"),
    ),
    (
        "0010_outbound_api",
        include_str!("../migrations/0010_outbound_api.sql"),
    ),
];

/// Bounds on every way a pool checkout can block. Without these a stalled
/// database — or a pool whose slots are all held by slow queries — makes
/// `get()` wait forever, so one slow dependency piles up the whole request
/// queue until the process is wedged. Each bound turns that hang into a
/// prompt, retryable error instead.
#[derive(Clone, Copy, Debug)]
pub struct PoolTimeouts {
    /// TCP connect + startup handshake cap, applied to the postgres config.
    pub connect: Duration,
    /// Cap on waiting for a free slot when the pool is already at `max_size`.
    pub wait: Duration,
    /// Cap on creating a brand-new connection (the manager's `create`).
    pub create: Duration,
    /// Cap on validating/recycling an idle connection before it is reused.
    pub recycle: Duration,
}

impl Default for PoolTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            wait: Duration::from_secs(10),
            create: Duration::from_secs(10),
            recycle: Duration::from_secs(2),
        }
    }
}

/// Builds a pool from a `postgres://user:pass@host:port/db` URL with the
/// default [`PoolTimeouts`].
pub fn build_pool(database_url: &str, max_size: usize) -> Result<Pool, StoreError> {
    build_pool_with_timeouts(database_url, max_size, PoolTimeouts::default())
}

/// Builds a pool with explicit checkout bounds. Production uses the defaults;
/// the override exists so a test can prove a bound actually fires (an exhausted
/// pool times out in milliseconds rather than ten seconds) and so an operator
/// can later tune the values from configuration without touching this code.
pub fn build_pool_with_timeouts(
    database_url: &str,
    max_size: usize,
    timeouts: PoolTimeouts,
) -> Result<Pool, StoreError> {
    let mut pg_config: tokio_postgres::Config = database_url
        .parse()
        .map_err(|err: tokio_postgres::Error| StoreError::Config(err.to_string()))?;
    pg_config.connect_timeout(timeouts.connect);

    let manager = Manager::from_config(
        pg_config,
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );

    Pool::builder(manager)
        .max_size(max_size)
        // deadpool needs an explicit runtime to schedule the timeout futures;
        // without it, building a pool with any timeout set fails outright
        // ("Timeouts require a runtime"). The whole stack runs on tokio.
        .runtime(Runtime::Tokio1)
        .wait_timeout(Some(timeouts.wait))
        .create_timeout(Some(timeouts.create))
        .recycle_timeout(Some(timeouts.recycle))
        .build()
        .map_err(|err| StoreError::Pool(err.to_string()))
}

/// Applies every pending migration. Idempotent and safe to call on startup.
pub async fn migrate(pool: &Pool) -> Result<(), StoreError> {
    let mut client = pool
        .get()
        .await
        .map_err(|err| StoreError::Pool(err.to_string()))?;

    let tx = client.transaction().await?;

    // Serialise migration across processes. Released automatically at commit.
    tx.batch_execute("SELECT pg_advisory_xact_lock(727274)")
        .await?;
    let table_exists: bool = tx
        .query_one(
            "SELECT to_regclass('public.schema_version') IS NOT NULL",
            &[],
        )
        .await?
        .get(0);

    if !table_exists {
        tx.batch_execute(
            "CREATE TABLE schema_version (
                 version    TEXT PRIMARY KEY,
                 applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
             )",
        )
        .await?;
    }

    for (version, sql) in MIGRATIONS {
        let applied: bool = tx
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM schema_version WHERE version = $1)",
                &[version],
            )
            .await?
            .get(0);

        if applied {
            continue;
        }

        tracing::info!(version, "applying migration");
        tx.batch_execute(sql).await?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES ($1)",
            &[version],
        )
        .await?;
    }

    tx.commit().await?;
    Ok(())
}
