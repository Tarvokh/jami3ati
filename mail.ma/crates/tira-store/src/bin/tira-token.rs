//! `tira-token` — mint, list, and revoke read-API bearer tokens.
//!
//! The secret is printed **once**, to stdout, so it can be piped to a secret
//! manager. Everything else (ids, scope, expiry) goes to stderr and can be
//! shown in a terminal. The control plane stores only the SHA-256.
//!
//! ```text
//! $env:TIRA_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
//! tira-token --tenant <uuid> --label "pilot-uit" --ttl-days 90
//! tira-token --tenant <uuid> --list
//! tira-token --revoke <token-id>
//! ```

use std::process::ExitCode;

use chrono::Duration;
use tira_store::PgStore;
use uuid::Uuid;

const USAGE: &str = "\
tira-token — issue, list, and revoke Tira Mail read-API tokens

USAGE:
  tira-token --tenant <uuid> [--mailbox <uuid>] [--label <text>]
             [--ttl-days <n> | --no-expiry]
  tira-token --tenant <uuid> --list
  tira-token --revoke <token-id>
  tira-token --purge-dead [--grace-days <n>]

ENVIRONMENT:
  TIRA_DATABASE_URL   required, e.g. postgres://postgres@127.0.0.1:55432/tira_test
  TIRA_BLOB_ROOT      optional, default ./data/blobs (migrations need the store)

The secret is written to stdout once. Metadata goes to stderr.
";

#[derive(Default)]
struct Args {
    tenant: Option<String>,
    mailbox: Option<String>,
    label: Option<String>,
    ttl_days: Option<i64>,
    no_expiry: bool,
    list: bool,
    revoke: Option<String>,
    purge_dead: bool,
    grace_days: Option<i64>,
    help: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| -> Result<String, String> {
            it.next().ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--tenant" => args.tenant = Some(value("--tenant")?),
            "--mailbox" => args.mailbox = Some(value("--mailbox")?),
            "--label" => args.label = Some(value("--label")?),
            "--ttl-days" => {
                let raw = value("--ttl-days")?;
                args.ttl_days = Some(
                    raw.parse()
                        .map_err(|_| format!("--ttl-days must be an integer, got {raw}"))?,
                );
            }
            "--revoke" => args.revoke = Some(value("--revoke")?),
            "--purge-dead" => args.purge_dead = true,
            "--grace-days" => {
                let raw = value("--grace-days")?;
                args.grace_days = Some(
                    raw.parse()
                        .map_err(|_| format!("--grace-days must be an integer, got {raw}"))?,
                );
            }
            "--no-expiry" => args.no_expiry = true,
            "--list" => args.list = true,
            "-h" | "--help" => args.help = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn uuid_arg(raw: Option<String>, name: &str) -> Result<Uuid, String> {
    let raw = raw.ok_or_else(|| format!("{name} is required"))?;
    Uuid::parse_str(&raw).map_err(|_| format!("{name} must be a UUID, got {raw}"))
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    if args.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let database_url = match std::env::var("TIRA_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            eprintln!("error: TIRA_DATABASE_URL is not set\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let blob_root = std::env::var("TIRA_BLOB_ROOT").unwrap_or_else(|_| "./data/blobs".to_string());

    let store = match PgStore::connect(&database_url, &blob_root).await {
        Ok(store) => store,
        Err(err) => {
            eprintln!("error: cannot open store: {err}");
            return ExitCode::FAILURE;
        }
    };

    if args.purge_dead {
        let days = args.grace_days.unwrap_or(7);
        if days < 0 {
            eprintln!("error: --grace-days must be zero or positive");
            return ExitCode::from(2);
        }
        match store.purge_dead_tokens(Duration::days(days)).await {
            Ok(removed) => {
                eprintln!("purged {removed} dead token(s) (grace {days} day(s))");
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    if args.grace_days.is_some() {
        eprintln!("error: --grace-days only makes sense with --purge-dead");
        return ExitCode::from(2);
    }

    if let Some(raw) = args.revoke {
        let id = match Uuid::parse_str(&raw) {
            Ok(id) => id,
            Err(_) => {
                eprintln!("error: --revoke needs a token UUID, got {raw}");
                return ExitCode::from(2);
            }
        };
        match store.revoke_token(id).await {
            Ok(true) => {
                eprintln!("revoked token {id}");
                return ExitCode::SUCCESS;
            }
            Ok(false) => {
                eprintln!("no live token with id {id} (already revoked or unknown)");
                return ExitCode::FAILURE;
            }
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    if args.list {
        let tenant = match uuid_arg(args.tenant, "--tenant") {
            Ok(id) => id,
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::from(2);
            }
        };
        let tokens = match store.list_tokens(tenant).await {
            Ok(tokens) => tokens,
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        };
        if tokens.is_empty() {
            eprintln!("no tokens for tenant {tenant}");
            return ExitCode::SUCCESS;
        }
        println!(
            "{:<38}  {:<20}  {:<38}  {:<12}  last used",
            "id", "label", "mailbox", "expires"
        );
        for token in tokens {
            let mailbox = token
                .mailbox_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "(tenant-wide)".to_string());
            let expires = token
                .expires_at
                .map(|at| at.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "never".to_string());
            let last = token
                .last_used_at
                .map(|at| at.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "never".to_string());
            println!(
                "{:<38}  {:<20}  {:<38}  {:<12}  {}",
                token.id, token.label, mailbox, expires, last
            );
        }
        return ExitCode::SUCCESS;
    }

    // ---- issue ---------------------------------------------------------
    let tenant = match uuid_arg(args.tenant, "--tenant") {
        Ok(id) => id,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    let mailbox = match args.mailbox {
        Some(raw) => match Uuid::parse_str(&raw) {
            Ok(id) => Some(id),
            Err(_) => {
                eprintln!("error: --mailbox must be a UUID, got {raw}");
                return ExitCode::from(2);
            }
        },
        None => None,
    };
    let label = args.label.unwrap_or_else(|| "read-api".to_string());

    let ttl = if args.no_expiry {
        None
    } else {
        let days = args.ttl_days.unwrap_or(90);
        if days <= 0 {
            eprintln!("error: --ttl-days must be positive, or pass --no-expiry");
            return ExitCode::from(2);
        }
        Some(Duration::days(days))
    };

    match store.issue_token(tenant, mailbox, &label, ttl).await {
        Ok(issued) => {
            let record = issued.record;
            eprintln!("token id : {}", record.id);
            eprintln!("tenant   : {}", record.tenant_id);
            eprintln!(
                "mailbox  : {}",
                record
                    .mailbox_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "(tenant-wide)".to_string())
            );
            eprintln!("label    : {}", record.label);
            eprintln!(
                "expires  : {}",
                record
                    .expires_at
                    .map(|at| at.to_rfc3339())
                    .unwrap_or_else(|| "never".to_string())
            );
            eprintln!("secret is printed once on stdout; store it now, it cannot be recovered");
            println!("{}", issued.secret);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
