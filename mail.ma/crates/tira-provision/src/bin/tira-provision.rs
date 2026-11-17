//! `tira-provision` — one mailbox at a time, by design.
//!
//! A bulk "provision everything" command cannot work safely: the engine needs a
//! plaintext password at creation time, and the control plane deliberately does
//! not store it. So the operator supplies the password once, on stdin, and the
//! process links exactly one mailbox.
//!
//! ```text
//! $env:TIRA_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
//! $env:TIRA_PROVISION_PASSWORD = "..."   # or type it when prompted
//! tira-provision --mailbox 33333333-3333-4333-8333-333333333333
//! tira-provision --list-pending
//! tira-provision --mailbox <id> --dry-run   # prints the plan, masks the secret
//! tira-provision --reconcile                # read-only drift report, exit 2 on drift
//! tira-provision --reconcile --fix          # demote drifted rows to failed
//! ```

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use tira_provision::plan::{account_upsert, domain_upsert, mask_secret, to_ndjson};
use tira_provision::{Provisioner, StalwartCli};
use tira_store::PgStore;
use uuid::Uuid;

type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Distinct from `1` (a command failed) so cron/CI can alert on drift while
/// still treating a broken database or engine as a hard failure.
const EXIT_DRIFT: u8 = 2;

struct Args {
    database_url: String,
    blob_root: String,
    mailbox: Option<Uuid>,
    list_pending: bool,
    dry_run: bool,
    reconcile: bool,
    fix: bool,
    limit: i64,
    password: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("tira-provision: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> CliResult<ExitCode> {
    let args = Args::parse()?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let store = PgStore::connect(&args.database_url, &args.blob_root).await?;

        if args.list_pending {
            let pending = store.unprovisioned_mailboxes(100).await?;
            if pending.is_empty() {
                println!("no unprovisioned mailboxes");
                return Ok(ExitCode::SUCCESS);
            }
            for mailbox in pending {
                println!(
                    "{}\t{}\t{}",
                    mailbox.mailbox_id,
                    mailbox.address(),
                    mailbox.engine_state.as_str()
                );
            }
            return Ok(ExitCode::SUCCESS);
        }

        if args.reconcile {
            let engine = StalwartCli::from_env()?;
            let provisioner = Provisioner::new(store, engine);
            let report = provisioner.reconcile(args.fix, args.limit).await?;

            println!(
                "reconcile: {} checked, {} in sync, {} drifted{}",
                report.checked,
                report.in_sync,
                report.drifted,
                if args.fix {
                    " (drifted rows marked failed)"
                } else {
                    ""
                }
            );
            for entry in report.drifted_entries() {
                println!(
                    "drift\t{}\t{}\trecorded={}\tengine={}",
                    entry.drift.as_str(),
                    entry.address,
                    entry.expected_engine_id.as_deref().unwrap_or("none"),
                    entry.actual_engine_id.as_deref().unwrap_or("none"),
                );
            }

            return Ok(if report.is_healthy() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(EXIT_DRIFT)
            });
        }

        let mailbox_id = args.mailbox.ok_or_else(|| {
            invalid("--mailbox <uuid>, --list-pending or --reconcile is required")
        })?;

        if args.dry_run {
            let target = store.provision_target(mailbox_id).await?;
            let plan = [
                domain_upsert(&target),
                account_upsert(&target, "dry-run-secret"),
            ];
            let masked: Vec<_> = plan.iter().map(mask_secret).collect();
            print!("{}", to_ndjson(&masked)?);
            eprintln!(
                "dry run for {} ({}); nothing was sent to the engine",
                target.address(),
                target.engine_state.as_str()
            );
            return Ok(ExitCode::SUCCESS);
        }

        let password = match args.password {
            Some(value) => value,
            None => read_password()?,
        };

        let engine = StalwartCli::from_env()?;
        let provisioner = Provisioner::new(store, engine);
        let outcome = provisioner.provision(mailbox_id, &password).await?;

        if outcome.already_active {
            println!(
                "already active: {} (engine id {})",
                outcome.address, outcome.engine_id
            );
        } else {
            println!(
                "provisioned: {} (engine id {})",
                outcome.address, outcome.engine_id
            );
        }
        Ok(ExitCode::SUCCESS)
    })
}

impl Args {
    fn parse() -> CliResult<Self> {
        let database_url = std::env::var("TIRA_DATABASE_URL").ok();
        let mut blob_root =
            std::env::var("TIRA_BLOB_ROOT").unwrap_or_else(|_| "./data/blobs".to_string());
        let mut mailbox = None;
        let mut list_pending = false;
        let mut dry_run = false;
        let mut reconcile = false;
        let mut fix = false;
        let mut limit: i64 = 10_000;
        let password = std::env::var("TIRA_PROVISION_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty());

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--database-url" => {
                    return Err(invalid(
                        "refusing --database-url on the command line (it leaks the credential in \
                         the process list); use TIRA_DATABASE_URL",
                    ))
                }
                "--blob-root" => blob_root = next_value(&mut args, "--blob-root")?,
                "--mailbox" => {
                    let value = next_value(&mut args, "--mailbox")?;
                    mailbox =
                        Some(Uuid::parse_str(&value).map_err(|_| {
                            invalid(format!("invalid UUID for --mailbox: {value}"))
                        })?);
                }
                "--list-pending" => list_pending = true,
                "--dry-run" => dry_run = true,
                "--reconcile" => reconcile = true,
                "--fix" => fix = true,
                "--limit" => {
                    let value = next_value(&mut args, "--limit")?;
                    limit = value
                        .parse::<i64>()
                        .map_err(|_| invalid(format!("invalid number for --limit: {value}")))?;
                    if limit <= 0 {
                        return Err(invalid("--limit must be greater than zero"));
                    }
                }
                "--password" => {
                    return Err(invalid(
                        "refusing --password on the command line (it leaks in the process list); \
                         use TIRA_PROVISION_PASSWORD or stdin",
                    ))
                }
                "-h" | "--help" => {
                    println!(
                        "usage: tira-provision --mailbox <uuid> [--dry-run]\n       \
                         tira-provision --list-pending\n       \
                         tira-provision --reconcile [--fix] [--limit N]\n\n\
                         env: TIRA_DATABASE_URL, TIRA_BLOB_ROOT, TIRA_PROVISION_PASSWORD,\n     \
                         STALWART_CLI, STALWART_URL, STALWART_USER, STALWART_PASSWORD,\n     \
                         STALWART_INSECURE=1 (loopback recovery only; remote endpoints require https)\n\n\
                         exit codes: 0 ok, 1 command failed, 2 reconcile found drift"
                    );
                    std::process::exit(0);
                }
                other => return Err(invalid(format!("unknown argument {other:?} (try --help)"))),
            }
        }

        if fix && !reconcile {
            return Err(invalid("--fix only makes sense with --reconcile"));
        }

        let database_url = database_url
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("TIRA_DATABASE_URL is not set"))?;

        Ok(Self {
            database_url,
            blob_root,
            mailbox,
            list_pending,
            dry_run,
            reconcile,
            fix,
            limit,
            password,
        })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> CliResult<String> {
    args.next()
        .ok_or_else(|| invalid(format!("{flag} needs a value")))
}

fn invalid(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

fn read_password() -> CliResult<String> {
    eprint!("one-time password for the mailbox (input is echoed): ");
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        return Err(invalid("empty password; nothing was provisioned"));
    }
    Ok(password)
}
