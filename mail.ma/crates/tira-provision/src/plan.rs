//! Stalwart bulk-plan encoding.
//!
//! Stalwart 1.0.2 accepts NDJSON where each line is one operation. The encoding
//! rules are not obvious and cost us real time to discover (see
//! `docs/CORRECTIONS.md` §8):
//!
//! - `Map<T>` is written as an object keyed by item, e.g. `{"0": {...}}`.
//! - `List` is written as an object with numeric keys, not an array.
//! - Plan-local references use `#key` and are only valid inside the same plan.
//! - `upsert` needs `matchOn`; without it the engine cannot be idempotent.
//!
//! Account names are only unique *per domain*. Matching on `name` alone would
//! relink `aicha@uca.ma` to `aicha@um5.ac.ma` the moment both exist, so the plan
//! matches on `["name", "domainId"]` — verified live on 5 Nov 2026.

use serde::Serialize;
use serde_json::{json, Map, Value};
use tira_store::PendingMailbox;
use uuid::Uuid;

/// Stable, plan-local reference for a domain. Not the engine's id.
pub fn domain_ref(domain_id: Uuid) -> String {
    format!("dom-{domain_id}")
}

/// Stable, plan-local reference for an account. Not the engine's id.
pub fn account_ref(mailbox_id: Uuid) -> String {
    format!("usr-{mailbox_id}")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanLine {
    #[serde(rename = "@type")]
    pub op: &'static str,
    pub object: &'static str,
    #[serde(rename = "matchOn", skip_serializing_if = "Option::is_none")]
    pub match_on: Option<Vec<&'static str>>,
    pub value: Value,
}

/// Creates or updates the domain that owns this mailbox.
///
/// The domain is upserted on every provisioning run, not only the first one.
/// That is deliberate: it repairs a domain deleted out of band without needing
/// a separate "ensure domain" command.
pub fn domain_upsert(mailbox: &PendingMailbox) -> PlanLine {
    let mut value = Map::new();
    value.insert(
        domain_ref(mailbox.domain_id),
        json!({
            "name": mailbox.domain,
            "description": format!("Tira Mail tenant {}", mailbox.tenant_id),
            "certificateManagement": { "@type": "Manual" },
            "dkimManagement": { "@type": "Automatic" },
            "dnsManagement": { "@type": "Manual" },
            "subAddressing": { "@type": "Enabled" }
        }),
    );

    PlanLine {
        op: "upsert",
        object: "Domain",
        match_on: Some(vec!["name"]),
        value: Value::Object(value),
    }
}

/// Creates or updates the account, with a one-time password.
///
/// The caller is responsible for dropping `secret` immediately after the plan
/// is applied. It must never be logged; [`PlanLine`]'s `Debug` output is the
/// reason this module exposes [`mask_secret`].
pub fn account_upsert(mailbox: &PendingMailbox, secret: &str) -> PlanLine {
    let mut credentials = Map::new();
    credentials.insert(
        "0".to_string(),
        json!({ "@type": "Password", "secret": secret }),
    );

    let mut value = Map::new();
    value.insert(
        account_ref(mailbox.mailbox_id),
        json!({
            "@type": "User",
            "name": mailbox.local_part,
            "domainId": format!("#{}", domain_ref(mailbox.domain_id)),
            "credentials": credentials,
            "description": mailbox
                .display_name
                .clone()
                .unwrap_or_else(|| mailbox.address()),
        }),
    );

    PlanLine {
        op: "upsert",
        object: "Account",
        match_on: Some(vec!["name", "domainId"]),
        value: Value::Object(value),
    }
}

/// Replaces the secret in a plan line with `***`, for `--dry-run` output.
///
/// Returns a modified clone so the real secret is never rendered by accident.
pub fn mask_secret(line: &PlanLine) -> PlanLine {
    let mut masked = line.clone();
    mask_value(&mut masked.value);
    masked
}

fn mask_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "secret" {
                    *child = Value::String("***".to_string());
                } else {
                    mask_value(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask_value),
        _ => {}
    }
}

/// Collects every non-empty `secret` string value in the plan (H-06).
///
/// The engine adapter uses these to redact the child's stderr before it can be
/// stored or printed: if `stalwart-cli` echoes a malformed plan record on a parse
/// error, the one-time password would otherwise leak into PostgreSQL and the
/// terminal. Mirrors [`mask_value`]'s walk, but gathers instead of masking.
pub fn secret_values(lines: &[PlanLine]) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        collect_secret_values(&line.value, &mut out);
    }
    out
}

fn collect_secret_values(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "secret" {
                    if let Some(s) = child.as_str() {
                        if !s.is_empty() {
                            out.push(s.to_string());
                        }
                    }
                } else {
                    collect_secret_values(child, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| collect_secret_values(v, out)),
        _ => {}
    }
}

/// Serialises a plan as NDJSON. Every line ends with `\n`, including the last:
/// `stalwart-cli` rejects a file whose final record is not newline-terminated.
pub fn to_ndjson(lines: &[PlanLine]) -> Result<String, serde_json::Error> {
    let mut out = String::new();
    for line in lines {
        out.push_str(&serde_json::to_string(line)?);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tira_store::EngineState;

    fn mailbox() -> PendingMailbox {
        PendingMailbox {
            mailbox_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
            tenant_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            domain_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            domain: "uca.ma".to_string(),
            local_part: "aicha".to_string(),
            display_name: Some("عائشة".to_string()),
            quota_bytes: 5_368_709_120,
            engine_state: EngineState::Pending,
            engine_id: None,
        }
    }

    #[test]
    fn account_matches_on_name_and_domain() {
        let line = account_upsert(&mailbox(), "Salam2026!");
        assert_eq!(line.object, "Account");
        assert_eq!(line.match_on.as_deref(), Some(&["name", "domainId"][..]));
    }

    #[test]
    fn account_references_the_domain_inside_the_same_plan() {
        let line = account_upsert(&mailbox(), "Salam2026!");
        let account = line.value.get(account_ref(mailbox().mailbox_id)).unwrap();
        assert_eq!(
            account.get("domainId").unwrap().as_str().unwrap(),
            format!("#{}", domain_ref(mailbox().domain_id))
        );
        assert_eq!(account.get("name").unwrap().as_str().unwrap(), "aicha");
        assert_eq!(
            account.get("credentials").unwrap().get("0").unwrap()["@type"],
            "Password"
        );
    }

    #[test]
    fn arabic_display_name_survives_serialisation() {
        let line = account_upsert(&mailbox(), "Salam2026!");
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("عائشة"));
    }

    #[test]
    fn plan_is_two_newline_terminated_records() {
        let plan = [domain_upsert(&mailbox()), account_upsert(&mailbox(), "x")];
        let ndjson = to_ndjson(&plan).unwrap();
        assert!(ndjson.ends_with('\n'));
        assert_eq!(ndjson.lines().count(), 2);
        for line in ndjson.lines() {
            let parsed: Value = serde_json::from_str(line).unwrap();
            assert_eq!(parsed["@type"], "upsert");
        }
    }

    #[test]
    fn masking_removes_every_secret() {
        let line = account_upsert(&mailbox(), "Salam2026!");
        let masked = mask_secret(&line);
        let text = serde_json::to_string(&masked).unwrap();
        assert!(!text.contains("Salam2026!"));
        assert!(text.contains("***"));
    }

    #[test]
    fn domain_is_upserted_on_name_only() {
        let line = domain_upsert(&mailbox());
        assert_eq!(line.match_on.as_deref(), Some(&["name"][..]));
        assert_eq!(
            line.value[&domain_ref(mailbox().domain_id)]["name"],
            "uca.ma"
        );
    }

    #[test]
    fn secret_values_collects_the_one_time_password() {
        let plan = [
            domain_upsert(&mailbox()),
            account_upsert(&mailbox(), "Salam2026!"),
        ];
        assert_eq!(secret_values(&plan), vec!["Salam2026!".to_string()]);
    }

    #[test]
    fn secret_values_is_empty_for_a_domain_only_plan() {
        let plan = [domain_upsert(&mailbox())];
        assert!(secret_values(&plan).is_empty());
    }
}
