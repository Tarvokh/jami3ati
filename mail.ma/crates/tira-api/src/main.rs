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
