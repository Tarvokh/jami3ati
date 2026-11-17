//! Live evidence for the control plane, served as JSON.
//!
//! This endpoint is deliberately **not** a compliance badge. It reads the
//! artifacts that were produced by the real tooling, re-derives the
//! cryptographic checks in Rust, probes the running database, and reports what
//! it observed. Nothing in the payload is a constant that says "compliant":
//!
//! * DKIM keys are decoded from the exported zone. The RSA modulus is read
//!   from the SPKI DER and measured in bits; the Ed25519 key is measured in
//!   bytes. A key shorter than 2048 bits is reported as a failure.
//! * The DMARC policy is parsed from the `_dmarc` TXT record. The
//!   `dmarcPolicy` field in the artifact JSON is only the *requested* script
//!   parameter (`engine` by default) and is never presented as the effective
//!   policy.
//! * Tenant isolation is not inferred from a migration file. A transaction
//!   drops to `tira_app`, sets a random tenant GUC, and counts mailboxes.
//!   A non-zero count fails the proof.
//! * The UI is rendered by the same function `/` serves and scanned for
//!   external references. XML namespace URIs (`xmlns=...`) are counted and
//!   explicitly excluded: they are names, not network requests.
//!
//! Failure policy:
//!
//! * `503` only when evidence is *unavailable* (no database, artifact missing,
//!   artifact unparseable). Retrying can help.
//! * `200` with `"status":"fail"` when evidence exists but a guarantee does
//!   not hold. Retrying cannot help; the deployment is wrong.
//!
//! The endpoint writes nothing. In particular it does not append an audit row
//! about itself, because self-auditing would make "audit is active" circular.
//! The audit write path is proven by an insert that is rolled back before the
//! response is built.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AppState;

/// An artifact older than this is reported as a warning. Seven days matches
/// the audit review cadence; DNS records do not change hourly.
const STALE_AFTER_SECONDS: i64 = 7 * 24 * 60 * 60;

/// How many offending snippets to include when an external reference is found.
const MAX_EXAMPLES: usize = 8;

/// Where the proof endpoint looks for evidence, and how it describes itself.
#[derive(Clone, Debug)]
pub struct ProofConfig {
    /// Directory holding `<domain>.dns.json` and `<domain>.zone`.
    pub dir: PathBuf,
    /// Domain whose artifact is re-verified.
    pub domain: String,
    /// Reported verbatim as `control_plane.environment`. Never inferred.
    pub environment: String,
    /// Sanitized classification of the database host, not its address.
    pub database_host_class: String,
}

impl Default for ProofConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("deploy/dns"),
            domain: "tira.test".to_string(),
            environment: "development".to_string(),
            database_host_class: "unknown".to_string(),
        }
    }
}

impl ProofConfig {
    /// Reads `TIRA_PROOF_DIR`, `TIRA_PROOF_DOMAIN`, and `TIRA_ENV`.
    /// Empty values are treated as absent so a blank variable in a compose file
    /// cannot silently point at `/`.
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Some(dir) = env_non_empty("TIRA_PROOF_DIR") {
            config.dir = PathBuf::from(dir);
        }
        if let Some(domain) = env_non_empty("TIRA_PROOF_DOMAIN") {
            config.domain = domain.to_ascii_lowercase();
        }
        if let Some(environment) = env_non_empty("TIRA_ENV") {
            config.environment = environment;
        }
        config
    }

    /// Records only a classification of the database host. The URL itself —
    /// which may contain a password — never reaches the response.
    pub fn with_database_url(mut self, database_url: &str) -> Self {
        self.database_host_class = host_class(database_url).to_string();
        self
    }
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Classifies a PostgreSQL URL without exposing it.
///
/// This is a proxy measurement, not a jurisdiction claim: `loopback` says the
/// kernel route is local, nothing about where the machine physically is.
pub fn host_class(database_url: &str) -> &'static str {
    let authority = database_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(database_url);
    let authority = authority.split('/').next().unwrap_or(authority);
    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    let host = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        host.split(':').next().unwrap_or(host)
    };

    match host.parse::<IpAddr>() {
        Ok(ip) if ip.is_loopback() => "loopback",
        Ok(IpAddr::V4(v4)) if v4.is_private() => "private_network",
        Ok(_) => "non_loopback",
        Err(_) => match host.to_ascii_lowercase().as_str() {
            "" => "unknown",
            "localhost" => "loopback",
            _ => "named_host",
        },
    }
}

// ---------------------------------------------------------------------------
// HTTP surface
// ---------------------------------------------------------------------------

/// Server-side TTL for the cached evidence payload. The endpoint is loopback
/// only (see `loopback_only` in the router), but a local poller — or a
/// compromised local process — must not be able to demand an unbounded number
/// of expensive rebuilds (artifact reads + audit COUNT queries + an RLS probe).
/// Thirty seconds keeps a dashboard fresh while bounding the rebuild rate.
pub const PROOF_CACHE_TTL: Duration = Duration::from_secs(30);

/// Shared, TTL-bounded cache of the last successfully built payload, stored as
/// the exact serialized JSON [`Bytes`]. `Arc` so every cloned [`AppState`] sees
/// the same entry; `tokio::sync::Mutex` so the guard can be held across the
/// async rebuild, serializing concurrent cold requests into a single build
/// instead of a thundering herd. Caching bytes (not the struct) means a hit is a
/// cheap ref-count clone, is byte-identical to the miss, and needs neither
/// `ProofResponse: Clone` nor serde's `rc` feature. Errors are never cached:
/// unavailable evidence may be available on retry.
pub type ProofCache = Arc<tokio::sync::Mutex<Option<(Instant, Bytes)>>>;

/// A fresh, empty cache for [`AppState`].
pub fn new_proof_cache() -> ProofCache {
    Arc::new(tokio::sync::Mutex::new(None))
}

/// `GET /api/v1/control-plane-proof` (aliased at `/api/v1/sovereignty-proof`).
///
/// Loopback-only at the router; this handler adds the server-side TTL cache.
/// `no-store` is kept so no browser or proxy retains the sensitive payload —
/// the cache is in-process only, and `generated_at` honestly reflects when the
/// evidence was gathered (up to `PROOF_CACHE_TTL` ago on a hit).
pub async fn control_plane_proof(State(state): State<AppState>) -> Response {
    let mut cache = state.proof_cache.lock().await;
    if let Some((built_at, bytes)) = cache.as_ref() {
        if built_at.elapsed() < PROOF_CACHE_TTL {
            return no_store(json_response(bytes.clone()));
        }
    }
    match build_proof(&state).await {
        Ok(body) => {
            // Serialize once; the cache holds these exact bytes so a hit neither
            // rebuilds nor re-serializes. ProofResponse is plain derived data
            // with no floats, so this cannot fail in practice — the fallback
            // preserves axum's standard Json error path if it ever does.
            match serde_json::to_vec(&body) {
                Ok(bytes) => {
                    let bytes = Bytes::from(bytes);
                    *cache = Some((Instant::now(), bytes.clone()));
                    no_store(json_response(bytes))
                }
                Err(err) => {
                    tracing::warn!(%err, "proof payload could not be serialized for caching");
                    no_store((StatusCode::OK, Json(body)).into_response())
                }
            }
        }
        Err(err) => {
            tracing::warn!(?err, "proof endpoint could not gather live evidence");
            err.into_response()
        }
    }
}

/// Builds a JSON response from already-serialized bytes, with the same
/// `content-type` axum's `Json` sets. Used for cache hits and misses alike so
/// the two are byte-identical.
fn json_response(bytes: Bytes) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        bytes,
    )
        .into_response()
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}

#[derive(Debug)]
pub(crate) enum ProofError {
    /// Evidence is missing or unreadable. Distinct from a failed check, which
    /// is a `200` with `"status":"fail"`.
    Unavailable(&'static str),
}

impl IntoResponse for ProofError {
    fn into_response(self) -> Response {
        let ProofError::Unavailable(reason) = self;
        no_store(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "status": "unavailable",
                    "error": reason,
                })),
            )
                .into_response(),
        )
    }
}

// ---------------------------------------------------------------------------
// Response contract
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ProofResponse {
    pub generated_at: DateTime<Utc>,
    /// `pass` when every check passed or warned; `fail` when one failed.
    pub status: &'static str,
    pub scope: Scope,
    pub control_plane: ControlPlane,
    pub mail_authentication: MailAuth,
    pub no_external_assets: ExternalAssets,
    /// Flat list so a reader (or a judge) can see exactly what was checked.
    pub checks: Vec<Check>,
}

#[derive(Debug, Serialize)]
pub struct Scope {
    pub proves: Vec<&'static str>,
    pub does_not_prove: Vec<&'static str>,
    pub artifact_provenance: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ControlPlane {
    pub cndp_law: &'static str,
    pub environment: String,
    pub database: DatabaseEvidence,
    pub tenant_isolation: IsolationEvidence,
    pub crown_jewels: CrownJewelsEvidence,
    pub audit: AuditEvidence,
}

#[derive(Debug, Serialize)]
pub struct DatabaseEvidence {
    pub connected: bool,
    pub host_class: String,
}

#[derive(Debug, Serialize)]
pub struct IsolationEvidence {
    pub verified: bool,
    pub method: &'static str,
    pub role: String,
    pub bypassrls: bool,
    pub random_tenant_rows: i64,
}

#[derive(Debug, Serialize)]
pub struct CrownJewelsEvidence {
    pub api_tokens_select: bool,
    pub api_tokens_insert: bool,
    pub message_recipients_select: bool,
    pub message_recipients_insert: bool,
    pub deny_all_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct AuditEvidence {
    pub write_path_verified: bool,
    pub insert_probe: &'static str,
    pub last_event_at: Option<DateTime<Utc>>,
    pub events_last_7d: i64,
}

#[derive(Debug, Serialize)]
pub struct MailAuth {
    pub domain: String,
    pub artifact: ArtifactInfo,
    pub zone: ZoneInfo,
    pub spf: Option<String>,
    pub dkim: Vec<DkimKey>,
    pub dmarc: Option<DmarcRecord>,
    pub mx: Option<String>,
    pub mta_sts: Option<String>,
    pub tls_rpt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactInfo {
    pub name: String,
    pub sha256: String,
    pub generated_at: DateTime<Utc>,
    pub age_seconds: i64,
    pub stale: bool,
}

#[derive(Debug, Serialize)]
pub struct ZoneInfo {
    pub name: String,
    pub sha256: String,
    pub records: usize,
}

#[derive(Debug, Serialize)]
pub struct DkimKey {
    pub selector: String,
    /// `rsa`, `ed25519`, or whatever the record declared.
    pub algo: String,
    pub key_bits: Option<u32>,
    pub key_bytes: Option<usize>,
    pub verified: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct DmarcRecord {
    pub record: String,
    pub policy: String,
    /// `monitoring` (p=none), `staged` (p=quarantine), `final` (p=reject).
    pub stage: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ExternalAssets {
    pub scanned: &'static str,
    pub bytes: usize,
    /// XML namespace URIs that a naive `http://` scan would flag.
    pub namespace_uris_ignored: usize,
    pub external_references: usize,
    pub examples: Vec<String>,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub status: &'static str,
    pub detail: String,
}

impl Check {
    fn pass(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            status: "pass",
            detail: detail.into(),
        }
    }

    fn warn(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            status: "warn",
            detail: detail.into(),
        }
    }

    fn fail(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            status: "fail",
            detail: detail.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Building the proof
// ---------------------------------------------------------------------------

async fn build_proof(state: &AppState) -> Result<ProofResponse, ProofError> {
    let store = state.store.as_ref().ok_or(ProofError::Unavailable(
        "no database configured: the control plane cannot be verified",
    ))?;
    let config = &state.proof;
    let domain = validate_domain(&config.domain)?;

    let json_name = format!("{domain}.dns.json");
    let zone_name = format!("{domain}.zone");
    let json_bytes = read_evidence(&config.dir, &json_name).await?;
    let zone_bytes = read_evidence(&config.dir, &zone_name).await?;

    let artifact: DnsArtifact = serde_json::from_slice(strip_bom(&json_bytes))
        .map_err(|_| ProofError::Unavailable("DNS artifact is not valid JSON"))?;
    let zone_text = String::from_utf8(zone_bytes.clone())
        .map_err(|_| ProofError::Unavailable("DNS zone artifact is not UTF-8"))?;
    // PowerShell 5.1 writes `-Encoding UTF8` with a BOM. A BOM is an encoding
    // artifact, not a malformed artifact: strip it before parsing so a zone
    // exported on Windows is read the same way as one exported on Linux.
    let zone_text = zone_text.trim_start_matches('\u{feff}');
    let zone = parse_zone(zone_text);

    let generated_at = artifact.generated_at;
    let age_seconds = (Utc::now() - generated_at).num_seconds().max(0);
    let artifact_info = ArtifactInfo {
        name: json_name,
        sha256: sha256_hex(&json_bytes),
        generated_at,
        age_seconds,
        stale: age_seconds > STALE_AFTER_SECONDS,
    };
    let zone_info = ZoneInfo {
        name: zone_name,
        sha256: sha256_hex(&zone_bytes),
        records: zone.records.len(),
    };

    let live = store.live_proof().await.map_err(|err| {
        tracing::warn!(%err, "live database proof failed");
        ProofError::Unavailable("database probe failed")
    })?;

    let isolation_ok = live.role == "tira_app" && !live.bypassrls && live.random_tenant_rows == 0;
    let mut checks = Vec::new();
    checks.push(if isolation_ok {
        Check::pass(
            "database.rls_isolation",
            format!(
                "role `{}` saw {} mailbox(es) with a random tenant GUC and BYPASSRLS={}",
                live.role, live.random_tenant_rows, live.bypassrls
            ),
        )
    } else {
        Check::fail(
            "database.rls_isolation",
            format!(
                "expected role tira_app, 0 rows, BYPASSRLS=false; got role=`{}`, rows={}, BYPASSRLS={}",
                live.role, live.random_tenant_rows, live.bypassrls
            ),
        )
    });

    let mut held_privileges = Vec::new();
    if live.crown_jewels.api_tokens_select {
        held_privileges.push("api_tokens SELECT");
    }
    if live.crown_jewels.api_tokens_insert {
        held_privileges.push("api_tokens INSERT");
    }
    if live.crown_jewels.message_recipients_select {
        held_privileges.push("message_recipients SELECT");
    }
    if live.crown_jewels.message_recipients_insert {
        held_privileges.push("message_recipients INSERT");
    }
    checks.push(if live.crown_jewels.deny_all_verified() {
        Check::pass(
            "database.crown_jewels",
            "tira_app holds no SELECT or INSERT privilege on api_tokens or message_recipients",
        )
    } else {
        Check::fail(
            "database.crown_jewels",
            format!(
                "tira_app holds privileges it must never hold: {}",
                held_privileges.join(", ")
            ),
        )
    });

    let (audit_check, write_path_verified, insert_probe) = match live.audit.insert_allowed {
        Some(true) => (
            Check::pass(
                "audit.write_path",
                "tira_app appended a row for its own tenant inside a transaction that was rolled back",
            ),
            true,
            "verified",
        ),
        Some(false) => (
            Check::fail(
                "audit.write_path",
                "tira_app cannot append an audit row for its own tenant",
            ),
            false,
            "rejected",
        ),
        None => (
            Check::warn(
                "audit.write_path",
                "no tenant exists yet, so the audit write path could not be probed",
            ),
            false,
            "skipped_no_tenant",
        ),
    };
    checks.push(audit_check);

    let (mail, mail_checks) = verify_mail(domain, &zone, &artifact, artifact_info, zone_info);
    checks.extend(mail_checks);

    let html = tira_frontend::asset_scan_target();
    let scan = scan_external_refs(&html);
    checks.push(if scan.references.is_empty() {
        Check::pass(
            "ui.no_external_assets",
            format!(
                "scanned {} bytes of rendered HTML; {} xml namespace URI(s) ignored; 0 external references",
                scan.bytes, scan.namespace_uris_ignored
            ),
        )
    } else {
        Check::fail(
            "ui.no_external_assets",
            format!(
                "{} external reference(s) found; first: {}",
                scan.references.len(),
                scan.references.first().cloned().unwrap_or_default()
            ),
        )
    });

    let status = if checks.iter().any(|check| check.status == "fail") {
        "fail"
    } else {
        "pass"
    };

    let external_references = scan.references.len();
    let external_verified = scan.references.is_empty();

    Ok(ProofResponse {
        generated_at: Utc::now(),
        status,
        scope: Scope {
            proves: vec![
                "tenant_isolation_enforced_by_postgresql_rls",
                "crown_jewel_tables_denied_to_the_application_role",
                "audit_write_path_available_to_the_application_role",
                "dkim_keys_reverified_from_the_exported_zone",
                "spf_dmarc_mx_mta_sts_tls_rpt_present_in_the_exported_zone",
                "no_external_asset_references_in_the_rendered_ui",
            ],
            does_not_prove: vec![
                "hosting_jurisdiction",
                "physical_data_residency",
                "public_dns_propagation",
                "port_25_egress",
                "acme_certificate_validity",
                "rspamd_runtime",
                "deliverability_to_major_providers",
                "artifact_signing",
            ],
            artifact_provenance:
                "unsigned; sha256 binds the exact bytes served, not the generator's honesty",
        },
        control_plane: ControlPlane {
            cndp_law: "09-08",
            environment: config.environment.clone(),
            database: DatabaseEvidence {
                connected: true,
                host_class: config.database_host_class.clone(),
            },
            tenant_isolation: IsolationEvidence {
                verified: isolation_ok,
                method: "live_probe",
                role: live.role,
                bypassrls: live.bypassrls,
                random_tenant_rows: live.random_tenant_rows,
            },
            crown_jewels: CrownJewelsEvidence {
                api_tokens_select: live.crown_jewels.api_tokens_select,
                api_tokens_insert: live.crown_jewels.api_tokens_insert,
                message_recipients_select: live.crown_jewels.message_recipients_select,
                message_recipients_insert: live.crown_jewels.message_recipients_insert,
                deny_all_verified: live.crown_jewels.deny_all_verified(),
            },
            audit: AuditEvidence {
                write_path_verified,
                insert_probe,
                last_event_at: live.audit.last_event_at,
                events_last_7d: live.audit.events_last_7d,
            },
        },
        mail_authentication: mail,
        no_external_assets: ExternalAssets {
            scanned: "tira_frontend::asset_scan_target",
            bytes: scan.bytes,
            namespace_uris_ignored: scan.namespace_uris_ignored,
            external_references,
            examples: scan.references,
            verified: external_verified,
        },
        checks,
    })
}

pub(crate) fn validate_domain(domain: &str) -> Result<&str, ProofError> {
    let valid = !domain.is_empty()
        && domain.len() <= 253
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && domain
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-');
    if valid {
        Ok(domain)
    } else {
        Err(ProofError::Unavailable(
            "TIRA_PROOF_DOMAIN is not a valid domain name",
        ))
    }
}

pub(crate) async fn read_evidence(dir: &Path, name: &str) -> Result<Vec<u8>, ProofError> {
    tokio::fs::read(dir.join(name)).await.map_err(|err| {
        tracing::warn!(file = name, %err, "proof artifact is unavailable");
        ProofError::Unavailable("required DNS artifact is missing or unreadable")
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Removes a UTF-8 byte-order mark, if present.
///
/// The hash exposed in the response still covers the exact bytes on disk; this
/// only affects parsing. Windows PowerShell writes `-Encoding UTF8` with a BOM,
/// and treating that as a malformed artifact would make the endpoint report an
/// outage for a perfectly valid export.
fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

// ---------------------------------------------------------------------------
// Artifact JSON
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DnsArtifact {
    domain: String,
    generated_at: DateTime<Utc>,
    records: usize,
    #[serde(default)]
    active_keys: Vec<DnsActiveKey>,
    #[serde(default)]
    findings: Vec<DnsFinding>,
}

#[derive(Debug, Deserialize)]
struct DnsActiveKey {
    selector: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct DnsFinding {
    level: String,
    code: String,
    #[serde(default)]
    message: String,
}

// ---------------------------------------------------------------------------
// Zone parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ZoneRecord {
    owner: String,
    rtype: String,
    rdata: String,
}

#[derive(Debug, Default)]
pub(crate) struct Zone {
    records: Vec<ZoneRecord>,
}

impl Zone {
    /// First TXT record at `owner` whose value starts with `prefix`
    /// (case-insensitive). Used for SPF/DMARC/MTA-STS/TLS-RPT.
    fn txt_starting_with(&self, owner: &str, prefix: &str) -> Option<String> {
        let prefix = prefix.to_ascii_lowercase();
        self.records.iter().find_map(|record| {
            if record.owner != owner || record.rtype != "TXT" {
                return None;
            }
            let value = txt_value(&record.rdata);
            value
                .to_ascii_lowercase()
                .starts_with(&prefix)
                .then_some(value)
        })
    }

    fn txt(&self, owner: &str) -> Option<String> {
        self.records
            .iter()
            .find(|record| record.owner == owner && record.rtype == "TXT")
            .map(|record| txt_value(&record.rdata))
    }

    fn first(&self, owner: &str, rtype: &str) -> Option<&str> {
        self.records
            .iter()
            .find(|record| record.owner == owner && record.rtype == rtype)
            .map(|record| record.rdata.as_str())
    }

    fn dkim_record(&self, domain: &str, selector: &str) -> Option<String> {
        let owner = format!(
            "{}._domainkey.{}",
            selector.to_ascii_lowercase(),
            domain.to_ascii_lowercase()
        );
        self.txt(&owner)
    }

    /// Every `*._domainkey.<domain>` TXT record that declares `v=DKIM1`.
    fn dkim_keys(&self, domain: &str) -> Vec<DkimKey> {
        let suffix = format!("._domainkey.{}", domain.to_ascii_lowercase());
        self.records
            .iter()
            .filter(|record| record.rtype == "TXT" && record.owner.ends_with(&suffix))
            .filter_map(|record| {
                let value = txt_value(&record.rdata);
                if !value.to_ascii_lowercase().starts_with("v=dkim1") {
                    return None;
                }
                let selector = record.owner[..record.owner.len() - suffix.len()].to_string();
                let algo = tag_value(&value, "k")
                    .unwrap_or_else(|| "rsa".to_string())
                    .to_ascii_lowercase();
                let mut key = DkimKey {
                    selector,
                    algo: algo.clone(),
                    key_bits: None,
                    key_bytes: None,
                    verified: false,
                    detail: String::new(),
                };

                match algo.as_str() {
                    "rsa" => match tag_value(&value, "p") {
                        None => key.detail = "record has no p= value".to_string(),
                        Some(encoded) => match rsa_modulus_bits(&encoded) {
                            Ok(bits) => {
                                key.key_bits = Some(bits);
                                key.verified = bits >= 2048;
                                key.detail = format!("RSA modulus is {bits} bits");
                            }
                            Err(err) => key.detail = err,
                        },
                    },
                    "ed25519" => match tag_value(&value, "p") {
                        None => key.detail = "record has no p= value".to_string(),
                        Some(encoded) => match decode_base64(&encoded) {
                            Ok(raw) => {
                                key.key_bytes = Some(raw.len());
                                key.verified = raw.len() == 32;
                                key.detail = format!("Ed25519 key is {} bytes", raw.len());
                            }
                            Err(err) => key.detail = err,
                        },
                    },
                    other => key.detail = format!("unsupported key type `{other}`"),
                }

                Some(key)
            })
            .collect()
    }
}

/// Parses the small subset of DNS zone syntax Stalwart emits: one logical
/// record per line, with multi-line TXT grouped by parentheses and quoted
/// strings concatenated.
pub(crate) fn parse_zone(text: &str) -> Zone {
    let mut zone = Zone::default();
    let mut origin = String::new();
    let mut buffer = String::new();
    let mut depth = 0i32;

    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if buffer.is_empty() && line.starts_with('$') {
            if let Some(rest) = line.strip_prefix("$ORIGIN") {
                origin = rest.trim().trim_end_matches('.').to_ascii_lowercase();
            }
            continue;
        }

        if !buffer.is_empty() {
            buffer.push(' ');
        }
        buffer.push_str(line);

        let (opens, closes) = paren_balance(line);
        depth += opens - closes;
        if depth > 0 {
            continue;
        }

        let record = buffer.trim().to_string();
        buffer.clear();
        depth = 0;
        push_record(&mut zone, &record, &origin);
    }

    if !buffer.trim().is_empty() {
        push_record(&mut zone, buffer.trim(), &origin);
    }

    zone
}

fn push_record(zone: &mut Zone, text: &str, origin: &str) {
    let mut rest = text;
    let owner_raw = next_word(&mut rest);
    if owner_raw.is_empty() {
        return;
    }

    let mut rtype = String::new();
    loop {
        let word = next_word(&mut rest);
        if word.is_empty() {
            break;
        }
        if word.eq_ignore_ascii_case("IN") || word.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        rtype = word.to_ascii_uppercase();
        break;
    }
    if rtype.is_empty() {
        return;
    }

    zone.records.push(ZoneRecord {
        owner: normalize_owner(owner_raw, origin),
        rtype,
        rdata: rest.trim().to_string(),
    });
}

fn next_word<'a>(rest: &mut &'a str) -> &'a str {
    let trimmed = rest.trim_start();
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let word = &trimmed[..end];
    *rest = &trimmed[end..];
    word
}

fn normalize_owner(raw: &str, origin: &str) -> String {
    let raw = raw.trim_end_matches('.');
    let owner = if raw == "@" {
        origin.to_string()
    } else if origin.is_empty() || raw.contains('.') {
        raw.to_string()
    } else {
        format!("{raw}.{origin}")
    };
    owner.to_ascii_lowercase()
}

fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => return &line[..index],
            _ => {}
        }
    }
    line
}

fn paren_balance(line: &str) -> (i32, i32) {
    let mut in_quotes = false;
    let mut opens = 0;
    let mut closes = 0;
    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '(' if !in_quotes => opens += 1,
            ')' if !in_quotes => closes += 1,
            _ => {}
        }
    }
    (opens, closes)
}

/// Concatenates every quoted string in the RDATA. Falls back to the raw text
/// when the record is unquoted.
fn txt_value(rdata: &str) -> String {
    let mut out = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in rdata.chars() {
        if escaped {
            if in_quotes {
                out.push(ch);
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            _ if in_quotes => out.push(ch),
            _ => {}
        }
    }
    if out.is_empty() {
        rdata.trim().to_string()
    } else {
        out
    }
}

fn tag_value(record: &str, tag: &str) -> Option<String> {
    record.split(';').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key.trim().eq_ignore_ascii_case(tag) {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        } else {
            None
        }
    })
}

fn dmarc_policy(record: &str) -> Option<String> {
    tag_value(record, "p").map(|policy| policy.to_ascii_lowercase())
}

/// Returns a problem description, or `None` when the SPF record is sane.
fn spf_problem(record: &str) -> Option<&'static str> {
    let lower = record.to_ascii_lowercase();
    if !lower.starts_with("v=spf1") {
        return Some("does not start with v=spf1");
    }
    if lower.contains("+all") {
        return Some("uses +all (an open sender policy)");
    }
    if !(lower.contains("-all") || lower.contains("~all")) {
        return Some("has no -all or ~all terminator");
    }
    None
}

// ---------------------------------------------------------------------------
// DKIM mathematics
// ---------------------------------------------------------------------------

fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|err| format!("p= is not valid base64: {err}"))
}

/// Reads the RSA modulus length from a base64 SubjectPublicKeyInfo.
///
/// This is the same measurement the Node/PowerShell validator does, repeated
/// here so the endpoint does not trust a string that says "RSA 2048 bit".
fn rsa_modulus_bits(spki_base64: &str) -> Result<u32, String> {
    let der = decode_base64(spki_base64)?;
    let mut outer = Der::new(&der);
    let spki = outer.element(0x30)?;

    let mut spki = Der::new(spki);
    let _algorithm = spki.element(0x30)?;
    let bit_string = spki.element(0x03)?;
    let Some((&unused, key_bytes)) = bit_string.split_first() else {
        return Err("BIT STRING is empty".to_string());
    };
    if unused != 0 {
        return Err("BIT STRING has unused bits".to_string());
    }

    let mut key = Der::new(key_bytes);
    let rsa = key.element(0x30)?;
    let mut rsa = Der::new(rsa);
    let modulus = rsa.element(0x02)?;

    let significant = modulus
        .iter()
        .position(|byte| *byte != 0)
        .map(|index| &modulus[index..])
        .unwrap_or(&[]);
    let Some(&top) = significant.first() else {
        return Err("RSA modulus is zero".to_string());
    };

    let mut bits = (significant.len() as u32 - 1) * 8;
    let mut top = top;
    while top > 0 {
        bits += 1;
        top >>= 1;
    }
    Ok(bits)
}

struct Der<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Der<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| "DER length overflow".to_string())?;
        if end > self.data.len() {
            return Err("DER value runs past the input".to_string());
        }
        let value = &self.data[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn element(&mut self, tag: u8) -> Result<&'a [u8], String> {
        let actual = *self
            .data
            .get(self.offset)
            .ok_or_else(|| "unexpected end of DER".to_string())?;
        if actual != tag {
            return Err(format!("expected DER tag {tag:#04x}, found {actual:#04x}"));
        }
        self.offset += 1;
        let len = self.length()?;
        self.take(len)
    }

    fn length(&mut self) -> Result<usize, String> {
        let first = *self
            .data
            .get(self.offset)
            .ok_or_else(|| "unexpected end of DER".to_string())?;
        self.offset += 1;
        if first < 0x80 {
            return Ok(first as usize);
        }
        let count = (first & 0x7f) as usize;
        if count == 0 || count > 4 {
            return Err("unsupported DER length encoding".to_string());
        }
        let mut len = 0usize;
        for _ in 0..count {
            let byte = *self
                .data
                .get(self.offset)
                .ok_or_else(|| "unexpected end of DER".to_string())?;
            self.offset += 1;
            len = (len << 8) | byte as usize;
        }
        Ok(len)
    }
}

/// Re-verifies an active selector against the algorithm the artifact claims.
/// This catches a mismatch between what the generator believed and what the
/// zone actually publishes.
fn verify_declared_key(kind: &str, record: &str) -> Result<(), String> {
    let kind = kind.to_ascii_lowercase();
    let encoded = tag_value(record, "p").ok_or_else(|| "record has no p= value".to_string())?;
    if kind.contains("rsa") {
        let bits = rsa_modulus_bits(&encoded)?;
        if bits < 2048 {
            return Err(format!("RSA modulus is {bits} bits (< 2048)"));
        }
    } else if kind.contains("ed25519") {
        let raw = decode_base64(&encoded)?;
        if raw.len() != 32 {
            return Err(format!("Ed25519 key is {} bytes (expected 32)", raw.len()));
        }
    } else {
        return Err(format!("unhandled key type `{kind}`"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mail authentication checks
// ---------------------------------------------------------------------------

/// Per-message mail-authentication view: loads the DNS artifacts for a domain
/// and re-runs the same checks the control-plane proof re-derives.
///
/// The reader UI uses this for the per-message authentication card (SPF/DKIM/
/// DMARC verdicts for the sender domain). It returns `None` when the evidence
/// for the domain is unavailable, so the UI renders the honest unavailable state
/// instead of claiming any verdict. Only the mail-authentication checks are
/// returned: the `artifact.*` checks describe the evidence itself and belong to
/// the sovereignty endpoint, not to a single message card.
pub(crate) async fn mail_authentication_for(
    state: &AppState,
    domain: &str,
) -> Option<(MailAuth, Vec<Check>)> {
    let config = &state.proof;
    let effective_domain = validate_domain(domain).ok()?;
    let json_name = format!("{effective_domain}.dns.json");
    let zone_name = format!("{effective_domain}.zone");
    let json_bytes = read_evidence(&config.dir, &json_name).await.ok()?;
    let zone_bytes = read_evidence(&config.dir, &zone_name).await.ok()?;
    let artifact: DnsArtifact = serde_json::from_slice(strip_bom(&json_bytes)).ok()?;
    let zone_text = String::from_utf8(zone_bytes.clone()).ok()?;
    let zone_text = zone_text.trim_start_matches('\u{feff}');
    let zone = parse_zone(zone_text);
    let artifact_info = ArtifactInfo {
        name: json_name,
        sha256: sha256_hex(&json_bytes),
        generated_at: artifact.generated_at,
        age_seconds: (Utc::now() - artifact.generated_at).num_seconds().max(0),
        stale: false,
    };
    let zone_info = ZoneInfo {
        name: zone_name,
        sha256: sha256_hex(&zone_bytes),
        records: zone.records.len(),
    };

    let (mail, mut checks) =
        verify_mail(effective_domain, &zone, &artifact, artifact_info, zone_info);
    checks.retain(|check| is_mail_auth_check(check.id));
    Some((mail, checks))
}

/// Which of the proof checks answer "did this domain's mail authentication
/// hold?". The `artifact.*` checks describe the evidence file itself; the
/// reader-facing card only renders the verdicts about the domain.
fn is_mail_auth_check(id: &str) -> bool {
    matches!(
        id,
        "spf" | "dkim.rsa" | "dkim.ed25519" | "dmarc" | "mx" | "mta_sts" | "tls_rpt"
    )
}

pub(crate) fn verify_mail(
    domain: &str,
    zone: &Zone,
    artifact: &DnsArtifact,
    artifact_info: ArtifactInfo,
    zone_info: ZoneInfo,
) -> (MailAuth, Vec<Check>) {
    let mut checks = Vec::new();

    checks.push(if artifact.domain.eq_ignore_ascii_case(domain) {
        Check::pass("artifact.domain", format!("artifact describes {domain}"))
    } else {
        Check::fail(
            "artifact.domain",
            format!(
                "artifact describes `{}`, but TIRA_PROOF_DOMAIN is `{domain}`",
                artifact.domain
            ),
        )
    });

    checks.push(if artifact.records == zone_info.records {
        Check::pass(
            "artifact.record_count",
            format!(
                "{} records in both the artifact and the parsed zone",
                zone_info.records
            ),
        )
    } else {
        Check::fail(
            "artifact.record_count",
            format!(
                "artifact claims {} records, the zone parses to {}",
                artifact.records, zone_info.records
            ),
        )
    });

    let mut missing = Vec::new();
    let mut unverified = Vec::new();
    for key in &artifact.active_keys {
        match zone.dkim_record(domain, &key.selector) {
            None => missing.push(key.selector.clone()),
            Some(record) => {
                if let Err(reason) = verify_declared_key(&key.kind, &record) {
                    unverified.push(format!("{}: {reason}", key.selector));
                }
            }
        }
    }
    checks.push(if artifact.active_keys.is_empty() {
        Check::fail(
            "artifact.active_keys",
            "the artifact lists no active DKIM selectors",
        )
    } else if missing.is_empty() && unverified.is_empty() {
        Check::pass(
            "artifact.active_keys",
            format!(
                "{} active selector(s) re-verified from the zone",
                artifact.active_keys.len()
            ),
        )
    } else {
        Check::fail(
            "artifact.active_keys",
            format!(
                "missing from the zone: [{}]; failing re-verification: [{}]",
                missing.join(", "),
                unverified.join(", ")
            ),
        )
    });

    let errors: Vec<&DnsFinding> = artifact
        .findings
        .iter()
        .filter(|finding| finding.level.eq_ignore_ascii_case("error"))
        .collect();
    let warnings: Vec<&DnsFinding> = artifact
        .findings
        .iter()
        .filter(|finding| finding.level.eq_ignore_ascii_case("warn"))
        .collect();
    checks.push(if !errors.is_empty() {
        Check::fail(
            "artifact.generator_findings",
            format!(
                "the generator recorded error(s): {}",
                describe_findings(&errors)
            ),
        )
    } else if !warnings.is_empty() {
        Check::warn(
            "artifact.generator_findings",
            format!(
                "the generator recorded warning(s): {}",
                describe_findings(&warnings)
            ),
        )
    } else {
        Check::pass(
            "artifact.generator_findings",
            format!(
                "{} generator finding(s), none at error level",
                artifact.findings.len()
            ),
        )
    });

    checks.push(if artifact_info.stale {
        Check::warn(
            "artifact.freshness",
            format!(
                "artifact is {} seconds old; regenerate it before showing this to anyone",
                artifact_info.age_seconds
            ),
        )
    } else {
        Check::pass(
            "artifact.freshness",
            format!("artifact is {} seconds old", artifact_info.age_seconds),
        )
    });

    // --- SPF -----------------------------------------------------------
    let spf = zone.txt_starting_with(domain, "v=spf1");
    checks.push(match &spf {
        None => Check::fail("spf", "no v=spf1 TXT record at the apex"),
        Some(record) => match spf_problem(record) {
            None => Check::pass("spf", record.clone()),
            Some(problem) => Check::fail("spf", format!("{problem}: {record}")),
        },
    });

    // --- MX ------------------------------------------------------------
    let mx = zone.first(domain, "MX").map(str::to_string);
    checks.push(match &mx {
        Some(record) => Check::pass("mx", format!("apex MX: {record}")),
        None => Check::fail("mx", "no MX record at the apex"),
    });

    // --- DMARC ---------------------------------------------------------
    let dmarc = zone
        .txt_starting_with(&format!("_dmarc.{domain}"), "v=dmarc1")
        .map(|record| {
            let policy = dmarc_policy(&record).unwrap_or_else(|| "unknown".to_string());
            let stage = match policy.as_str() {
                "none" => "monitoring",
                "quarantine" => "staged",
                "reject" => "final",
                _ => "unknown",
            };
            DmarcRecord {
                record,
                policy,
                stage,
            }
        });
    checks.push(match &dmarc {
        None => Check::fail("dmarc", "no _dmarc TXT record"),
        Some(record) => match record.policy.as_str() {
            "reject" | "quarantine" => Check::pass(
                "dmarc",
                format!("{} (stage: {})", record.record, record.stage),
            ),
            "none" => Check::warn(
                "dmarc",
                format!(
                    "policy is p=none (monitoring only, not enforcement): {}",
                    record.record
                ),
            ),
            other => Check::fail("dmarc", format!("unrecognised policy p={other}")),
        },
    });

    // --- DKIM ----------------------------------------------------------
    let dkim = zone.dkim_keys(domain);
    let rsa_keys: Vec<&DkimKey> = dkim.iter().filter(|key| key.algo == "rsa").collect();
    checks.push(if rsa_keys.is_empty() {
        Check::fail("dkim.rsa", "no RSA DKIM selector was found in the zone")
    } else if let Some(key) = rsa_keys.iter().find(|key| key.verified) {
        Check::pass("dkim.rsa", format!("{}: {}", key.selector, key.detail))
    } else {
        Check::fail(
            "dkim.rsa",
            format!(
                "no RSA selector reaches 2048 bits: {}",
                describe_keys(&rsa_keys)
            ),
        )
    });

    let ed25519_keys: Vec<&DkimKey> = dkim.iter().filter(|key| key.algo == "ed25519").collect();
    checks.push(if ed25519_keys.is_empty() {
        Check::fail(
            "dkim.ed25519",
            "no Ed25519 DKIM selector was found in the zone",
        )
    } else if let Some(key) = ed25519_keys.iter().find(|key| key.verified) {
        Check::pass("dkim.ed25519", format!("{}: {}", key.selector, key.detail))
    } else {
        Check::fail(
            "dkim.ed25519",
            format!(
                "no Ed25519 selector is exactly 32 bytes: {}",
                describe_keys(&ed25519_keys)
            ),
        )
    });

    // --- MTA-STS / TLS-RPT ---------------------------------------------
    let mta_sts = zone.txt_starting_with(&format!("_mta-sts.{domain}"), "v=stsv1");
    checks.push(match &mta_sts {
        Some(_) => Check::pass("mta_sts", "MTA-STS TXT record present"),
        None => Check::warn("mta_sts", "no _mta-sts TXT record"),
    });

    let tls_rpt = zone.txt_starting_with(&format!("_smtp._tls.{domain}"), "v=tlsrptv1");
    checks.push(match &tls_rpt {
        Some(_) => Check::pass("tls_rpt", "TLS-RPT TXT record present"),
        None => Check::warn("tls_rpt", "no _smtp._tls TLS-RPT record"),
    });

    (
        MailAuth {
            domain: domain.to_string(),
            artifact: artifact_info,
            zone: zone_info,
            spf,
            dkim,
            dmarc,
            mx,
            mta_sts,
            tls_rpt,
        },
        checks,
    )
}

fn describe_findings(findings: &[&DnsFinding]) -> String {
    findings
        .iter()
        .map(|finding| format!("{}: {}", finding.code, finding.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn describe_keys(keys: &[&DkimKey]) -> String {
    keys.iter()
        .map(|key| format!("{} ({})", key.selector, key.detail))
        .collect::<Vec<_>>()
        .join("; ")
}

// ---------------------------------------------------------------------------
// External-reference scan
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ExternalScan {
    bytes: usize,
    namespace_uris_ignored: usize,
    references: Vec<String>,
}

/// Finds asset references that would make the browser talk to a third party.
///
/// `xmlns="http://www.w3.org/2000/svg"` is an XML namespace identifier, not a
/// request. Counting it as a CDN would be a false positive, so namespace URIs
/// are counted and excluded before the patterns below run.
fn scan_external_refs(html: &str) -> ExternalScan {
    const PATTERNS: [&str; 10] = [
        "src=\"http",
        "src='http",
        "src=http",
        "href=\"http",
        "href='http",
        "href=http",
        "url(http",
        "url('http",
        "url(\"http",
        "@import",
    ];

    let lower = html.to_ascii_lowercase();
    let namespace_uris_ignored =
        lower.matches("xmlns=\"http").count() + lower.matches("xmlns:xlink=\"http").count();

    let mut hits: Vec<(usize, String)> = Vec::new();
    for pattern in PATTERNS {
        let mut from = 0;
        while let Some(relative) = lower[from..].find(pattern) {
            let start = from + relative;
            // Normalize to the offset of the `http` the pattern refers to, so
            // `@import url("https://…")` is one reference, not two. Counting
            // raw pattern hits would overstate the finding.
            let url_start = if pattern == "@import" {
                match lower[start..].find("http") {
                    Some(offset) if offset <= 200 => start + offset,
                    _ => start,
                }
            } else {
                start + pattern.len() - "http".len()
            };
            let mut end = (url_start + 60).min(html.len());
            while end < html.len() && !html.is_char_boundary(end) {
                end += 1;
            }
            hits.push((url_start, excerpt(&html[url_start..end])));
            from = start + pattern.len();
            if from >= lower.len() {
                break;
            }
        }
    }
    hits.sort_by_key(|(start, _)| *start);
    hits.dedup_by_key(|(start, _)| *start);

    ExternalScan {
        bytes: html.len(),
        namespace_uris_ignored,
        references: hits
            .into_iter()
            .take(MAX_EXAMPLES)
            .map(|(_, text)| text)
            .collect(),
    }
}

fn excerpt(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ").trim().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_ZONE: &str = include_str!("../../../deploy/dns/tira.test.zone");

    /// A syntactically valid SubjectPublicKeyInfo whose modulus is 1024 bits.
    /// It is a test fixture only: the endpoint must reject it.
    const WEAK_RSA_SPKI: &str = "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQCAMFV6n8TpDjNYfaLH7BE2W4Clyu8UOV6DqM3yFzxhhqvQ9Ro/ZImu0/gdQmeMsdb7IEVqj7TZ/iNIbZK33AEmS3CVut8EKU5zmL3iByxRdpvA5QovVHmew+gNMld8ocbrEDVaf6TJ7hM4XYKnzPEWO2CFqs/0GT5jiK3S9xxBZgIDAQAB";

    #[test]
    fn parses_the_real_tira_zone() {
        let zone = parse_zone(REAL_ZONE);
        // The zone legitimately grows when DKIM selectors are rotated: old and
        // new keys coexist during a rollover. A frozen equality would break on
        // every rotation, so assert a floor that still catches accidental
        // shrinkage/corruption, and pin the structural records below.
        assert!(
            zone.records.len() >= 24,
            "logical record count drifted below the known baseline: {}",
            zone.records.len()
        );

        assert_eq!(
            zone.txt_starting_with("tira.test", "v=spf1").as_deref(),
            Some("v=spf1 mx -all")
        );
        assert_eq!(
            zone.first("tira.test", "MX"),
            Some("10 mail.tira.test."),
            "apex MX not parsed"
        );
        assert_eq!(
            zone.txt("_dmarc.tira.test").as_deref(),
            Some("v=DMARC1; p=reject; rua=mailto:postmaster@tira.test")
        );
        assert_eq!(
            dmarc_policy("v=DMARC1; p=reject; rua=mailto:postmaster@tira.test").as_deref(),
            Some("reject")
        );
    }

    #[test]
    fn multi_line_dkim_record_is_concatenated() {
        let zone = parse_zone(REAL_ZONE);
        let record = zone
            .dkim_record("tira.test", "v1-rsa-20261104")
            .expect("RSA selector");
        assert!(
            record.starts_with("v=DKIM1; k=rsa; h=sha256; p=MIIBIjAN"),
            "{record}"
        );
        let encoded = tag_value(&record, "p").expect("p=");
        assert_eq!(rsa_modulus_bits(&encoded).expect("valid SPKI"), 2048);
    }

    #[test]
    fn ed25519_key_is_measured_in_bytes() {
        let zone = parse_zone(REAL_ZONE);
        let keys = zone.dkim_keys("tira.test");
        // Verify EVERY selector in the zone, not a frozen count: during a key
        // rollover old and new selectors coexist, and the proof endpoint
        // re-verifies all of them. This must hold for any number of keys, and
        // is stronger than the old single-key .find() check.
        assert!(
            keys.len() >= 2,
            "expected at least one RSA and one Ed25519 selector, got {}",
            keys.len()
        );
        assert!(
            keys.iter().any(|key| key.algo == "ed25519"),
            "no ed25519 selector in the zone"
        );
        assert!(
            keys.iter().any(|key| key.algo == "rsa"),
            "no rsa selector in the zone"
        );

        for key in keys.iter().filter(|key| key.algo == "ed25519") {
            assert_eq!(
                key.key_bytes,
                Some(32),
                "ed25519 selector {} is not 32 bytes",
                key.selector
            );
            assert!(
                key.verified,
                "ed25519 selector {} did not verify",
                key.selector
            );
        }
        for key in keys.iter().filter(|key| key.algo == "rsa") {
            assert_eq!(
                key.key_bits,
                Some(2048),
                "rsa selector {} is not 2048 bits",
                key.selector
            );
            assert!(key.verified, "rsa selector {} did not verify", key.selector);
        }
    }

    #[test]
    fn weak_rsa_key_is_rejected() {
        assert_eq!(rsa_modulus_bits(WEAK_RSA_SPKI).expect("parse"), 1024);
        assert!(
            verify_declared_key("Dkim1RsaSha256", &format!("v=DKIM1; p={WEAK_RSA_SPKI}")).is_err()
        );
    }

    #[test]
    fn utf8_bom_is_stripped_before_parsing() {
        const REAL_JSON: &[u8] = include_bytes!("../../../deploy/dns/tira.test.dns.json");

        assert_eq!(strip_bom(b"\xEF\xBB\xBF{}"), b"{}");
        assert_eq!(strip_bom(b"{}"), b"{}");

        // The committed artifact is written by PowerShell 5.1 with a BOM; it
        // must still parse, otherwise every Windows export looks like an outage.
        let artifact: DnsArtifact =
            serde_json::from_slice(strip_bom(REAL_JSON)).expect("real artifact parses");
        assert_eq!(artifact.domain, "tira.test");
        assert!(!artifact.active_keys.is_empty());
    }

    #[test]
    fn bom_does_not_hide_the_first_zone_record() {
        let raw = parse_zone(REAL_ZONE);
        let stripped = parse_zone(REAL_ZONE.trim_start_matches('\u{feff}'));
        assert_eq!(raw.records.len(), stripped.records.len());
        assert!(
            stripped
                .dkim_record("tira.test", "v1-ed25519-20261104")
                .is_some(),
            "the first record after a BOM must still be addressable by selector"
        );
    }

    #[test]
    fn malformed_spki_is_an_error_not_a_panic() {
        assert!(rsa_modulus_bits("not base64 !!!").is_err());
        assert!(rsa_modulus_bits("TUlJQklqQU5CZ2twaGtpRzR3MEJBUUVG").is_err());
    }

    #[test]
    fn external_scan_ignores_xml_namespaces() {
        let html = r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0"/></svg>"#;
        let scan = scan_external_refs(html);
        assert_eq!(scan.namespace_uris_ignored, 1);
        assert!(scan.references.is_empty(), "{:?}", scan.references);
    }

    #[test]
    fn external_scan_finds_real_requests() {
        let html = r#"<script src="https://cdn.example/x.js"></script>
<img src="http://tracker.example/p.gif">
<style>@import url("https://fonts.example/x.css");</style>"#;
        let scan = scan_external_refs(html);
        assert_eq!(scan.references.len(), 3, "{:?}", scan.references);
    }

    #[test]
    fn host_class_never_returns_the_address() {
        assert_eq!(
            host_class("postgres://user:secret@127.0.0.1:55432/tira_test"),
            "loopback"
        );
        assert_eq!(
            host_class("postgres://user:secret@[::1]:55432/tira_test"),
            "loopback"
        );
        assert_eq!(
            host_class("postgres://user:secret@10.0.0.5:5432/tira"),
            "private_network"
        );
        assert_eq!(
            host_class("postgres://user:secret@db.internal:5432/tira"),
            "named_host"
        );
        assert_eq!(
            host_class("postgres://user:secret@203.0.113.9:5432/tira"),
            "non_loopback"
        );
    }

    #[test]
    fn spf_problems_are_reported() {
        assert_eq!(spf_problem("v=spf1 mx -all"), None);
        assert_eq!(spf_problem("v=spf1 mx ~all"), None);
        assert!(spf_problem("v=spf1 +all").is_some());
        assert!(spf_problem("v=spf1 mx").is_some());
        assert!(spf_problem("v=spf2 mx -all").is_some());
    }

    #[test]
    fn zone_without_a_records_origin_is_still_parsed() {
        let zone = parse_zone(
            "example.test. IN MX 10 mail.example.test.\n\
             _dmarc.example.test. IN TXT \"v=DMARC1; p=quarantine\"\n",
        );
        assert_eq!(zone.records.len(), 2);
        assert_eq!(
            dmarc_policy(&zone.txt("_dmarc.example.test").expect("dmarc")).as_deref(),
            Some("quarantine")
        );
    }
}
