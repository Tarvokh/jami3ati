//! `stalwart-cli` adapter.
//!
//! We shell out to the official CLI instead of reimplementing Stalwart's
//! management HTTP API. That keeps the bridge small and lets Stalwart evolve
//! without us chasing its schema. The cost is operational: the CLI must be
//! installed next to the service, and a failure to spawn it is a provisioning
//! failure, not a silent skip.
//!
//! This adapter never logs the password or the plan body, and neither secret
//! ever touches the process argument list or the filesystem:
//!
//! * The admin credential (`STALWART_URL`/`USER`/`PASSWORD`) is handed to the
//!   child through its environment, which `stalwart-cli` reads natively. argv is
//!   world-readable in the process list (`ps`, Task Manager, `/proc`), the
//!   environment of a child we spawn is not.
//! * The plan carries a one-time mailbox password, so it is piped to
//!   `apply --stdin` instead of being written to a temp file. There is no disk
//!   artifact to leak, mis-permission, or leave behind on a crash — on any OS.

use std::path::PathBuf;
use std::process::Stdio;

use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::plan::{secret_values, to_ndjson, PlanLine};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("missing environment variable {0}")]
    MissingEnv(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to encode plan: {0}")]
    Plan(#[from] serde_json::Error),
    #[error("stalwart-cli apply failed (exit {code}): {stderr}")]
    Apply { code: i32, stderr: String },
    #[error("stalwart-cli query failed (exit {code}): {stderr}")]
    Query { code: i32, stderr: String },
    #[error("domain {0:?} is missing from the engine after apply")]
    DomainNotFound(String),
    #[error("account {0:?} is missing from the engine after apply")]
    AccountNotFound(String),
    /// M-12: a zero exit code is not proof of success; the engine must return a
    /// summary, report no failures, and account for every plan line we sent.
    #[error("stalwart-cli apply produced no summary; refusing to report success")]
    NoSummary,
    #[error("stalwart-cli apply reported {failed} failed operation(s) despite a zero exit code")]
    ApplyFailed { failed: u64 },
    #[error(
        "stalwart-cli apply accounted for {got} operation(s) but {expected} plan line(s) were sent"
    )]
    IncompleteApply { expected: u64, got: u64 },
    /// M-13: `--where` values are interpolated, so they must stay inside a safe
    /// alphabet or a crafted local part/domain could resolve another account.
    #[error(
        "refusing unsafe --where filter value {0:?}: only letters, digits and . _ + - are allowed"
    )]
    InvalidFilterValue(String),
    /// M-15: certificate verification (or plaintext HTTP) may only be relaxed
    /// for a loopback recovery listener, never for a remote management endpoint.
    #[error("refusing insecure engine endpoint {url:?}: non-loopback endpoints must use https with certificate verification")]
    InsecureEndpoint { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StalwartConfig {
    /// Path to `stalwart-cli` (or just the name if it is on `PATH`).
    pub binary: PathBuf,
    /// Management endpoint, e.g. `https://mail.tira.test` or the recovery
    /// listener `http://127.0.0.1:8080`.
    pub url: String,
    pub user: String,
    pub password: String,
    /// Accept a self-signed certificate (`--insecure`).
    pub insecure: bool,
}

impl StalwartConfig {
    /// Reads `STALWART_CLI`, `STALWART_URL`, `STALWART_USER`,
    /// `STALWART_PASSWORD`, `STALWART_INSECURE`.
    pub fn from_env() -> Result<Self, EngineError> {
        let user = std::env::var("STALWART_USER")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| EngineError::MissingEnv("STALWART_USER".to_string()))?;
        let password = std::env::var("STALWART_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| EngineError::MissingEnv("STALWART_PASSWORD".to_string()))?;

        let config = Self {
            binary: std::env::var_os("STALWART_CLI")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("stalwart-cli")),
            url: std::env::var("STALWART_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string()),
            user,
            password,
            insecure: std::env::var("STALWART_INSECURE").as_deref() == Ok("1"),
        };
        config.validate()?;
        Ok(config)
    }

    /// M-15: a remote management endpoint must be HTTPS with certificate
    /// verification on. Only a loopback recovery listener may use plaintext
    /// HTTP or a self-signed certificate (`STALWART_INSECURE=1`); otherwise the
    /// admin credential would cross the network in the clear or to an
    /// unauthenticated peer. Fails fast at construction and again before any
    /// process is spawned, so a misconfiguration cannot reach the wire.
    pub fn validate(&self) -> Result<(), EngineError> {
        let host = url_host(&self.url).unwrap_or("");
        let loopback = host == "localhost" || host == "::1" || host.starts_with("127.");
        if loopback {
            return Ok(());
        }
        if !self.url.starts_with("https://") || self.insecure {
            return Err(EngineError::InsecureEndpoint {
                url: self.url.clone(),
            });
        }
        Ok(())
    }
}

/// Extracts the host from a management URL without a full URL parser: strip the
/// scheme, take the authority up to the first `/`, drop any `user@` userinfo,
/// unwrap a bracketed IPv6 literal, and drop the `:port`. Returns `None` only
/// for an empty authority, which [`StalwartConfig::validate`] treats as remote
/// (and therefore refuses unless it is HTTPS with verification).
fn url_host(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split('/').next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    if let Some(bracketed) = host_port.strip_prefix('[') {
        return bracketed.split(']').next();
    }
    host_port.split(':').next()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub created: u64,
    pub updated: u64,
    pub destroyed: u64,
    pub failed: u64,
}

/// A point-in-time read of the engine's identity tables.
///
/// Reconciliation must not spawn two CLI processes per mailbox, so the engine
/// answers with two queries and the comparison happens in memory:
///   * `domains`: lowercase domain name -> engine domain id
///   * `accounts`: (lowercase local part, lowercase domain) -> engine account id
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineSnapshot {
    pub domains: std::collections::HashMap<String, String>,
    pub accounts: std::collections::HashMap<(String, String), String>,
}

/// The two engine operations the bridge needs.
///
/// A trait (rather than a concrete type) keeps [`crate::service::Provisioner`]
/// testable without a running Stalwart: the integration test uses an in-memory
/// fake and still exercises the real PostgreSQL state machine.
#[async_trait::async_trait]
pub trait MailEngine: Send + Sync {
    async fn apply(&self, lines: &[PlanLine]) -> Result<ApplyReport, EngineError>;
    async fn lookup_account(
        &self,
        local_part: &str,
        domain: &str,
    ) -> Result<Option<String>, EngineError>;

    /// Reads every domain and account the engine knows about, in two queries.
    ///
    /// Used by `tira-provision --reconcile`; doing it per mailbox would spawn two
    /// CLI processes per row and turn a nightly job into an hour-long one.
    async fn snapshot(&self) -> Result<EngineSnapshot, EngineError>;
}

#[async_trait::async_trait]
impl MailEngine for StalwartCli {
    async fn apply(&self, lines: &[PlanLine]) -> Result<ApplyReport, EngineError> {
        StalwartCli::apply(self, lines).await
    }

    async fn lookup_account(
        &self,
        local_part: &str,
        domain: &str,
    ) -> Result<Option<String>, EngineError> {
        StalwartCli::lookup_account(self, local_part, domain).await
    }

    async fn snapshot(&self) -> Result<EngineSnapshot, EngineError> {
        StalwartCli::snapshot(self).await
    }
}

#[derive(Debug, Clone)]
pub struct StalwartCli {
    config: StalwartConfig,
}

impl StalwartCli {
    pub fn new(config: StalwartConfig) -> Self {
        Self { config }
    }

    pub fn from_env() -> Result<Self, EngineError> {
        Ok(Self::new(StalwartConfig::from_env()?))
    }

    /// Applies a plan and returns the engine's own summary.
    ///
    /// The plan is piped to `apply --stdin`; it is never written to disk. The
    /// one-time mailbox password lives only in this process's memory and the
    /// child's stdin pipe, so there is no temp file to leak or clean up — even
    /// if the child crashes or this future is cancelled mid-write.
    pub async fn apply(&self, lines: &[PlanLine]) -> Result<ApplyReport, EngineError> {
        if lines.is_empty() {
            return Ok(ApplyReport::default());
        }

        self.config.validate()?;
        let ndjson = to_ndjson(lines)?;
        let mut child = self.apply_command().spawn()?;

        // `apply_command` always pipes stdin; take it, write the plan, then drop
        // the handle to signal EOF so the CLI stops reading and starts applying.
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::Io(std::io::Error::other("child stdin was not piped")))?;
        stdin.write_all(ndjson.as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin);

        let output = child.wait_with_output().await?;
        // H-06: the CLI may echo a malformed plan record on a parse error, which
        // would carry the one-time mailbox password. Redact every plan secret
        // and the admin password before stderr can leave this module — it is
        // stored in PostgreSQL (`engine_error`) and printed to the terminal.
        let mut secrets = secret_values(lines);
        secrets.push(self.config.password.clone());
        if !output.status.success() {
            return Err(EngineError::Apply {
                code: output.status.code().unwrap_or(-1),
                stderr: redact_stderr(&output.stderr, &secrets),
            });
        }

        parse_summary(&output.stdout, lines.len())
    }

    /// Finds the engine id of `local_part@domain`, or `None`.
    ///
    /// `matchOn` upserts do not return the affected ids, so the bridge must ask.
    pub async fn lookup_account(
        &self,
        local_part: &str,
        domain: &str,
    ) -> Result<Option<String>, EngineError> {
        // M-13: these values are interpolated into `--where name=...`. Constrain
        // them to a safe alphabet first, or a crafted local part/domain (spaces,
        // '=', quotes) could inject filter syntax and resolve another account's
        // id. Validation happens before any process is spawned.
        validate_filter_token(local_part, 64, false)?;
        validate_filter_token(domain, 253, false)?;
        let domains = self
            .query("Domain", &[format!("name={domain}")], "id,name")
            .await?;
        let domain_id = domains
            .first()
            .and_then(|row| row.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::DomainNotFound(domain.to_string()))?;
        // Re-validate the engine-supplied id before interpolating it in turn.
        validate_filter_token(domain_id, 256, true)?;

        let accounts = self
            .query(
                "Account",
                &[
                    format!("name={local_part}"),
                    format!("domainId={domain_id}"),
                ],
                "id,emailAddress",
            )
            .await?;

        Ok(accounts
            .first()
            .and_then(|row| row.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    /// Runs `stalwart-cli query <object> --where ... --fields ... --json`.
    pub async fn query(
        &self,
        object: &str,
        filters: &[String],
        fields: &str,
    ) -> Result<Vec<Value>, EngineError> {
        self.config.validate()?;
        let mut command = self.base_command();
        command.arg("query").arg(object);
        for filter in filters {
            command.arg("--where").arg(filter);
        }
        command.arg("--fields").arg(fields).arg("--json");

        let output = command.output().await?;
        if !output.status.success() {
            return Err(EngineError::Query {
                code: output.status.code().unwrap_or(-1),
                stderr: redact_stderr(&output.stderr, std::slice::from_ref(&self.config.password)),
            });
        }

        let mut rows = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let line = line.trim();
            if !line.starts_with('{') {
                continue;
            }
            rows.push(serde_json::from_str(line)?);
        }
        Ok(rows)
    }

    /// Reads the engine's domains and accounts in exactly two queries.
    ///
    /// Names are lowercased because both sides of the bridge store canonical
    /// lowercase local parts and domains; comparing case-sensitively would
    /// report drift for accounts that are semantically identical.
    pub async fn snapshot(&self) -> Result<EngineSnapshot, EngineError> {
        let domain_rows = self.query("Domain", &[], "id,name").await?;
        let mut name_by_id: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut domains = std::collections::HashMap::new();
        for row in &domain_rows {
            let (Some(id), Some(name)) = (
                row.get("id").and_then(Value::as_str),
                row.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            let name = name.to_ascii_lowercase();
            name_by_id.insert(id.to_string(), name.clone());
            domains.insert(name, id.to_string());
        }

        let account_rows = self.query("Account", &[], "id,name,domainId").await?;
        let mut accounts = std::collections::HashMap::new();
        for row in &account_rows {
            let (Some(id), Some(name), Some(domain_id)) = (
                row.get("id").and_then(Value::as_str),
                row.get("name").and_then(Value::as_str),
                row.get("domainId").and_then(Value::as_str),
            ) else {
                continue;
            };
            let Some(domain) = name_by_id.get(domain_id) else {
                // An account pointing at a domain the engine no longer lists is
                // unusable; leaving it out makes reconciliation report it.
                continue;
            };
            accounts.insert((name.to_ascii_lowercase(), domain.clone()), id.to_string());
        }

        Ok(EngineSnapshot { domains, accounts })
    }

    /// Builds the shared CLI invocation.
    ///
    /// Credentials go through the child's environment, never argv: the process
    /// argument list is readable by any local user (`ps`, Task Manager,
    /// `/proc/<pid>/cmdline`), whereas a spawned child's environment is not
    /// exposed there. `stalwart-cli` reads `STALWART_URL`/`USER`/`PASSWORD`
    /// natively, so no `--url`/`--user`/`--password` flags are needed.
    fn base_command(&self) -> Command {
        let mut command = Command::new(&self.config.binary);
        command
            .env("STALWART_URL", &self.config.url)
            .env("STALWART_USER", &self.config.user)
            .env("STALWART_PASSWORD", &self.config.password)
            .stdin(Stdio::null());
        if self.config.insecure {
            command.arg("--insecure");
        }
        command
    }

    /// The `apply` invocation: reads the NDJSON plan from stdin, not a file.
    ///
    /// Split out from [`Self::base_command`] so the stdin contract (piped in,
    /// `--stdin` flag, no `--file`) is unit-testable without spawning a process.
    fn apply_command(&self) -> Command {
        let mut command = self.base_command();
        command
            .arg("apply")
            .arg("--stdin")
            .arg("--json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}

/// M-12: a zero exit code is not proof of success. The engine must emit a
/// `summary` line, report `failed == 0`, and account for at least as many
/// operations as the plan lines we sent. Anything else is a hard error rather
/// than a silent zero-filled "success" that would mark a mailbox active while
/// the engine never actually created it.
fn parse_summary(stdout: &[u8], expected_lines: usize) -> Result<ApplyReport, EngineError> {
    let mut summary: Option<Value> = None;
    for line in String::from_utf8_lossy(stdout).lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("op").and_then(Value::as_str) == Some("summary") {
            summary = Some(value);
        }
    }
    let Some(done) = summary.as_ref().and_then(|s| s.get("done")) else {
        return Err(EngineError::NoSummary);
    };
    let report = ApplyReport {
        created: done.get("created").and_then(Value::as_u64).unwrap_or(0),
        updated: done.get("updated").and_then(Value::as_u64).unwrap_or(0),
        destroyed: done.get("destroyed").and_then(Value::as_u64).unwrap_or(0),
        failed: done.get("failed").and_then(Value::as_u64).unwrap_or(0),
    };
    if report.failed > 0 {
        return Err(EngineError::ApplyFailed {
            failed: report.failed,
        });
    }
    let accounted = report.created + report.updated + report.destroyed;
    if accounted < expected_lines as u64 {
        return Err(EngineError::IncompleteApply {
            expected: expected_lines as u64,
            got: accounted,
        });
    }
    Ok(report)
}

/// M-13: `--where` filters are built by interpolating `name={value}`. Reject any
/// value outside a safe alphabet (lowercase letters/digits and `. _ + -`, plus
/// uppercase for engine-supplied ids) so spaces, `=`, quotes or other filter
/// metacharacters cannot smuggle in a second predicate and match another row.
fn validate_filter_token(
    value: &str,
    max_len: usize,
    allow_upper: bool,
) -> Result<(), EngineError> {
    let safe = !value.is_empty()
        && value.len() <= max_len
        && value.bytes().all(|b| {
            b.is_ascii_digit()
                || matches!(b, b'.' | b'_' | b'+' | b'-')
                || b.is_ascii_lowercase()
                || (allow_upper && b.is_ascii_uppercase())
        });
    if safe {
        Ok(())
    } else {
        Err(EngineError::InvalidFilterValue(value.to_string()))
    }
}

/// H-06: redact the child's stderr before it can leave this module. The CLI may
/// echo a malformed plan record on a parse error, carrying the one-time mailbox
/// password; this text is stored in PostgreSQL and printed to a terminal, so it
/// must be secret-free. Two layers: (1) remove every known secret value exactly
/// (longest first), then (2) drop any line still bearing a credential marker, in
/// case a transformed/partial echo escaped exact matching. Finally bound length
/// on a char boundary. Benign diagnostics survive, so failures stay debuggable.
fn redact_stderr(stderr: &[u8], secrets: &[String]) -> String {
    let mut redacted = String::from_utf8_lossy(stderr).into_owned();
    let mut ordered: Vec<&String> = secrets.iter().filter(|s| !s.is_empty()).collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in ordered {
        redacted = redacted.replace(secret.as_str(), "[REDACTED]");
    }
    let kept: Vec<&str> = redacted
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.contains("\"secret\"")
                || lower.contains("\"password\"")
                || lower.contains("\"credentials\"")
                || lower.contains("secret=")
                || lower.contains("password=")
            {
                "[REDACTED LINE]"
            } else {
                line
            }
        })
        .collect();
    let joined = kept.join("\n");
    let trimmed = joined.trim();
    if trimmed.len() <= 2000 {
        return trimmed.to_string();
    }
    let mut end = 2000;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &trimmed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config carrying a recognisable admin secret so the assertions can prove
    /// it never lands in argv. No process is spawned; these tests run anywhere.
    fn test_cli() -> StalwartCli {
        StalwartCli::new(StalwartConfig {
            binary: PathBuf::from("stalwart-cli"),
            url: "http://127.0.0.1:8080".to_string(),
            user: "admin@tira.test".to_string(),
            password: "ADMIN-SECRET-do-not-leak".to_string(),
            insecure: false,
        })
    }

    fn args_of(command: &Command) -> Vec<String> {
        command
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn admin_credential_never_appears_in_argv() {
        let cli = test_cli();
        let command = cli.base_command();
        let args = args_of(&command);

        assert!(
            !args.iter().any(|a| a.contains("ADMIN-SECRET-do-not-leak")),
            "the admin password must not be in the process argument list: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a == "--password"),
            "no --password flag: the secret travels via the environment"
        );
        assert!(
            !args.iter().any(|a| a == "--user" || a == "--url"),
            "identity/endpoint also travel via the environment, not argv"
        );

        let envs: Vec<(String, Option<String>)> = command
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert!(
            envs.iter().any(|(k, v)| k == "STALWART_PASSWORD"
                && v.as_deref() == Some("ADMIN-SECRET-do-not-leak")),
            "the password must be handed to the child through its environment"
        );
        assert!(envs.iter().any(|(k, _)| k == "STALWART_USER"));
        assert!(envs.iter().any(|(k, _)| k == "STALWART_URL"));
    }

    #[test]
    fn apply_reads_the_plan_from_stdin_not_a_disk_file() {
        let cli = test_cli();
        let args = args_of(&cli.apply_command());

        assert!(
            args.iter().any(|a| a == "--stdin"),
            "apply must read the plan from stdin: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a == "--file"),
            "apply must not reference a disk file (it would hold a one-time secret)"
        );
        assert!(
            !args.iter().any(|a| a.contains("ADMIN-SECRET-do-not-leak")),
            "the apply invocation must not leak the admin secret into argv either"
        );
    }

    #[test]
    fn parses_the_summary_line_and_ignores_progress_lines() {
        let stdout = br#"{"op":"upsert","object":"Domain","status":"ok"}
{"op":"summary","plan":{"upserts":2},"done":{"destroyed":0,"updated":1,"created":2,"failed":0}}
"#;
        let report = parse_summary(stdout, 2).expect("a complete, failure-free summary");
        assert_eq!(
            report,
            ApplyReport {
                created: 2,
                updated: 1,
                destroyed: 0,
                failed: 0
            }
        );
    }

    #[test]
    fn missing_summary_is_an_error_not_a_silent_zero() {
        // M-12: no summary must not masquerade as a zero-filled success.
        assert!(matches!(
            parse_summary(b"not json\n", 1),
            Err(EngineError::NoSummary)
        ));
        assert!(matches!(
            parse_summary(b"{\"op\":\"summary\"}\n", 1),
            Err(EngineError::NoSummary)
        ));
    }

    #[test]
    fn long_stderr_is_truncated_on_a_char_boundary() {
        let stderr = "خطأ ".repeat(1000);
        let short = redact_stderr(stderr.as_bytes(), &[]);
        assert!(short.ends_with('…'));
        assert!(short.len() <= 2000 + '…'.len_utf8());
    }

    // --- M-12: engine response validation ---

    #[test]
    fn failed_operations_with_exit_zero_are_an_error() {
        let stdout =
            br#"{"op":"summary","done":{"created":1,"updated":0,"destroyed":0,"failed":1}}"#;
        assert!(matches!(
            parse_summary(stdout, 2),
            Err(EngineError::ApplyFailed { failed: 1 })
        ));
    }

    #[test]
    fn incomplete_apply_is_an_error() {
        // Two plan lines sent, engine accounts for only one ⇒ a silent skip.
        let stdout =
            br#"{"op":"summary","done":{"created":1,"updated":0,"destroyed":0,"failed":0}}"#;
        assert!(matches!(
            parse_summary(stdout, 2),
            Err(EngineError::IncompleteApply {
                expected: 2,
                got: 1
            })
        ));
    }

    // --- M-13: filter injection ---

    #[test]
    fn filter_token_rejects_metacharacters() {
        for bad in [
            "a b", "a=b", "a\"b", "a'b", "a;b", "a|b", "", "a\nb", "UPPER",
        ] {
            assert!(
                matches!(
                    validate_filter_token(bad, 64, false),
                    Err(EngineError::InvalidFilterValue(_))
                ),
                "must reject {bad:?}"
            );
        }
    }

    #[test]
    fn filter_token_accepts_canonical_values() {
        for good in ["aicha", "aicha.n", "a+b", "a_b", "a-b", "uca.ma", "123"] {
            assert!(
                validate_filter_token(good, 253, false).is_ok(),
                "must accept {good:?}"
            );
        }
        // Engine-supplied ids may be uppercase/numeric.
        assert!(validate_filter_token("AbC-123_x", 256, true).is_ok());
    }

    #[test]
    fn filter_token_enforces_length() {
        let long = "a".repeat(65);
        assert!(matches!(
            validate_filter_token(&long, 64, false),
            Err(EngineError::InvalidFilterValue(_))
        ));
    }

    #[tokio::test]
    async fn lookup_rejects_unsafe_local_part_before_spawning() {
        let cli = test_cli();
        let err = cli.lookup_account("a b", "uca.ma").await.unwrap_err();
        assert!(matches!(err, EngineError::InvalidFilterValue(_)));
    }

    #[tokio::test]
    async fn lookup_rejects_unsafe_domain_before_spawning() {
        let cli = test_cli();
        let err = cli
            .lookup_account("aicha", "uca.ma name=x")
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::InvalidFilterValue(_)));
    }

    // --- M-15: insecure endpoint refusal ---

    fn config_with(url: &str, insecure: bool) -> StalwartConfig {
        StalwartConfig {
            binary: PathBuf::from("stalwart-cli"),
            url: url.to_string(),
            user: "admin@tira.test".to_string(),
            password: "ADMIN-SECRET-do-not-leak".to_string(),
            insecure,
        }
    }

    #[test]
    fn validate_refuses_insecure_on_a_remote_endpoint() {
        let cfg = config_with("https://mail.tira.test", true);
        assert!(matches!(
            cfg.validate(),
            Err(EngineError::InsecureEndpoint { .. })
        ));
    }

    #[test]
    fn validate_refuses_plaintext_http_on_a_remote_endpoint() {
        let cfg = config_with("http://mail.tira.test", false);
        assert!(matches!(
            cfg.validate(),
            Err(EngineError::InsecureEndpoint { .. })
        ));
    }

    #[test]
    fn validate_allows_loopback_http_and_insecure() {
        for url in [
            "http://127.0.0.1:8080",
            "http://localhost:8080",
            "http://[::1]:8080",
        ] {
            assert!(
                config_with(url, true).validate().is_ok(),
                "loopback {url} with a self-signed cert must be allowed"
            );
            assert!(config_with(url, false).validate().is_ok());
        }
    }

    #[test]
    fn validate_allows_remote_https_with_verification() {
        assert!(config_with("https://mail.tira.test", false)
            .validate()
            .is_ok());
    }

    #[test]
    fn url_host_parses_the_expected_forms() {
        assert_eq!(url_host("http://127.0.0.1:8080"), Some("127.0.0.1"));
        assert_eq!(url_host("https://mail.tira.test"), Some("mail.tira.test"));
        assert_eq!(
            url_host("https://mail.tira.test/path"),
            Some("mail.tira.test")
        );
        assert_eq!(url_host("http://[::1]:8080"), Some("::1"));
        assert_eq!(
            url_host("https://user@mail.tira.test"),
            Some("mail.tira.test")
        );
        assert_eq!(url_host("http://localhost:8080"), Some("localhost"));
    }

    // --- H-06: stderr redaction ---

    #[test]
    fn redact_removes_an_echoed_plan_secret() {
        let secret = "Salam-Aicha-2026!".to_string();
        let stderr = "parse error near: Salam-Aicha-2026! (rejected)";
        let out = redact_stderr(stderr.as_bytes(), &[secret]);
        assert!(!out.contains("Salam-Aicha-2026!"));
        assert!(out.contains("[REDACTED]"));
        assert!(out.contains("parse error"));
    }

    #[test]
    fn redact_drops_a_credential_line_even_if_the_secret_is_unknown() {
        // A transformed echo we cannot match exactly: the line still carries a
        // credential key, so it is dropped wholesale (defense in depth).
        let stderr = b"{\"@type\":\"Password\",\"secret\":\"hunter2\"}\nconnection reset";
        let out = redact_stderr(stderr, &[]);
        assert!(!out.contains("hunter2"));
        assert!(out.contains("[REDACTED LINE]"));
        assert!(out.contains("connection reset"));
    }

    #[test]
    fn redact_removes_the_admin_password() {
        let admin = "ADMIN-SECRET-do-not-leak".to_string();
        let stderr = b"authentication failed for ADMIN-SECRET-do-not-leak";
        let out = redact_stderr(stderr, &[admin]);
        assert!(!out.contains("ADMIN-SECRET-do-not-leak"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn redact_keeps_benign_diagnostics() {
        let out = redact_stderr(b"connection refused: 127.0.0.1:8080", &[]);
        assert_eq!(out, "connection refused: 127.0.0.1:8080");
    }
}
