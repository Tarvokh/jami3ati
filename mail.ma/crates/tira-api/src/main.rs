use std::net::SocketAddr;
use std::path::PathBuf;

use tira_api::{app, proof::ProofConfig, AppState};
use tira_smtp::server::{Server, ServerConfig};
use tira_store::PgStore;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let bind: SocketAddr = std::env::var("TIRA_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()?;

    // Persistence is opt-in: without TIRA_DATABASE_URL the process still runs
    // (useful for probing the API), but it refuses to accept mail, because
    // accepting mail with nowhere to put it is worse than not starting.
    let database_url = std::env::var("TIRA_DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty());

    // Serving real people from a database that the test suite writes into is
    // how this process ended up showing 3,743 fixtures as if they were tenants.
    // The guard is a name check, not a proof, but it stops the specific mistake
    // that already happened. A deliberate QA run can still opt in.
    if let Some(url) = database_url.as_deref() {
        if is_test_database(url) && !allow_test_database() {
            return Err(format!(
                "refusing to serve from a test database: {}. \
                 Point TIRA_DATABASE_URL at a non-test database (scripts/dev-local.ps1 creates `tira_dev`), \
                 or set TIRA_ALLOW_TEST_DB=1 to override for a deliberate QA run.",
                database_name(url)
                    .as_deref()
                    .unwrap_or("<unparsed>")
            )
            .into());
        }
    }

    // The proof endpoint describes where its evidence comes from. The database
    // host is reduced to a classification here so the URL (which may contain a
    // password) never enters the response.
    let mut proof = ProofConfig::from_env();
    if let Some(url) = database_url.as_deref() {
        proof = proof.with_database_url(url);
    }

    let mut state = match database_url.as_deref() {
        Some(url) => {
            let blob_root: PathBuf = std::env::var("TIRA_BLOB_ROOT")
                .unwrap_or_else(|_| "./data/blobs".to_string())
                .into();

            let store = PgStore::connect(url, &blob_root).await?;
            tracing::info!(blob_root = %blob_root.display(), "storage ready");

            // Dead tokens (expired or revoked) are pure bloat once past a short
            // retention window. Purge hourly so the table cannot grow without
            // bound across QA runs, rotations and revocations. The first tick
            // fires immediately at startup.
            //
            // The same tick sweeps orphaned blobs: bytes written by a delivery
            // whose transaction then rolled back. The 24h grace window is far
            // longer than any delivery transaction, so an in-flight commit can
            // never have its blob collected (see `purge_unreferenced_blobs`).
            let purge_store = store.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
                loop {
                    tick.tick().await;
                    match purge_store
                        .purge_dead_tokens(chrono::Duration::days(7))
                        .await
                    {
                        Ok(0) => {}
                        Ok(removed) => tracing::info!(removed, "dead api tokens purged"),
                        Err(err) => tracing::warn!(%err, "dead token purge failed"),
                    }
                    match purge_store
                        .purge_unreferenced_blobs(chrono::Duration::hours(24))
                        .await
                    {
                        Ok(0) => {}
                        Ok(removed) => tracing::info!(removed, "orphaned blobs purged"),
                        Err(err) => tracing::warn!(%err, "orphan blob purge failed"),
                    }
                }
            });

            // The manual tira-smtp server is DISABLED BY DEFAULT. Stalwart is
            // the production mail engine (SMTP/IMAP/JMAP); this in-process
            // server is a development/fallback path and an extra DoS surface,
            // so it only runs when explicitly opted in via TIRA_SMTP_ENABLE=1.
            // Even then it is hardened (bounded line reads, a DATA timeout, a
            // session cap) and refuses unknown recipients at RCPT (no relay).
            let smtp_enabled = std::env::var("TIRA_SMTP_ENABLE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

            let smtp_bound = if smtp_enabled {
                // 2525 by default: binding 25 needs privileges on Unix and is
                // usually already taken on Windows. Production overrides this.
                let smtp_addr: SocketAddr = std::env::var("TIRA_SMTP_ADDR")
                    .unwrap_or_else(|_| "127.0.0.1:2525".to_string())
                    .parse()?;
                let smtp = Server::new(ServerConfig::default(), store.clone());
                let smtp_listener = tokio::net::TcpListener::bind(smtp_addr).await?;
                let bound = smtp_listener.local_addr()?;
                tracing::info!(addr = %bound, "tira-smtp accepting mail (explicitly enabled)");

                tokio::spawn(async move {
                    if let Err(err) = smtp.serve_on(smtp_listener).await {
                        tracing::error!(%err, "smtp server stopped");
                    }
                });
                Some(bound)
            } else {
                tracing::info!(
                    "manual tira-smtp server disabled; set TIRA_SMTP_ENABLE=1 to run it (Stalwart is the production engine)"
                );
                None
            };

            AppState::with_store(env!("CARGO_PKG_VERSION"), store, smtp_bound)
        }
        None => {
            tracing::warn!(
                "TIRA_DATABASE_URL is not set: running without persistence, SMTP disabled"
            );
            AppState::new(env!("CARGO_PKG_VERSION"))
        }
    };
    state.proof = proof;

    // Outbound queue retry worker: replays durable sends that failed at their
    // first attempt, with exponential backoff, until delivered or dead-lettered.
    let outbound_worker_state = state.clone();
    tokio::spawn(async move {
        tira_api::send::outbound_worker(outbound_worker_state).await;
    });

    tracing::info!(
        proof_dir = %state.proof.dir.display(),
        proof_domain = %state.proof.domain,
        environment = %state.proof.environment,
        "live-evidence endpoint configured"
    );

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "tira-api listening");

    // `ConnectInfo` is what fills `RequestMeta.ip` for the audit trail and the
    // login limiter. Without it every event would record "unknown", and the
    // per-address throttle would collapse into one global bucket.
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

/// Extracts the database name from either a URL (`postgres://u@h:5432/db?x=1`)
/// or a key/value DSN (`host=h dbname=db`). Returns `None` when the shape is
/// not recognised, which the caller reports instead of guessing.
fn database_name(url: &str) -> Option<String> {
    if let Some(rest) = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))
    {
        let without_query = rest.split(['?', '#']).next().unwrap_or(rest);
        let name = without_query.rsplit('/').next()?;
        return (!name.is_empty()).then(|| name.to_string());
    }

    url.split_whitespace()
        .find_map(|token| token.strip_prefix("dbname="))
        .map(|name| name.trim_matches('\'').to_string())
        .filter(|name| !name.is_empty())
}

/// A database is "test" when its name ends in `_test`/`_tests`. That is the
/// convention every script and integration test in this repository uses.
fn is_test_database(url: &str) -> bool {
    database_name(url)
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with("_test") || lower.ends_with("_tests")
        })
        .unwrap_or(false)
}

fn allow_test_database() -> bool {
    std::env::var("TIRA_ALLOW_TEST_DB")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{database_name, is_test_database};

    #[test]
    fn reads_the_database_name_from_a_url() {
        assert_eq!(
            database_name("postgres://postgres@127.0.0.1:55432/tira_dev").as_deref(),
            Some("tira_dev")
        );
        assert_eq!(
            database_name("postgresql://u:p@h:5432/tira_test?sslmode=disable").as_deref(),
            Some("tira_test")
        );
    }

    #[test]
    fn reads_the_database_name_from_a_keyword_dsn() {
        assert_eq!(
            database_name("host=127.0.0.1 port=55432 dbname=tira_test user=postgres").as_deref(),
            Some("tira_test")
        );
        assert_eq!(database_name("host=127.0.0.1 user=postgres"), None);
    }

    #[test]
    fn flags_only_test_suffixed_databases() {
        assert!(is_test_database(
            "postgres://postgres@127.0.0.1:55432/tira_test"
        ));
        assert!(is_test_database(
            "postgres://postgres@127.0.0.1:55432/jami3ati_tests"
        ));
        assert!(!is_test_database(
            "postgres://postgres@127.0.0.1:55432/tira_dev"
        ));
        assert!(!is_test_database(
            "postgres://postgres@127.0.0.1:55432/tira"
        ));
        assert!(!is_test_database("not a dsn"));
    }
}
