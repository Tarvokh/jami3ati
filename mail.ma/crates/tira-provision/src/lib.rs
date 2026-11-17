//! The identity bridge between Tira's control plane and the mail engine.
//!
//! `tira-store` decides *who exists*. Stalwart decides *where mail lives*.
//! This crate is the only code allowed to write to both, so the two cannot
//! drift silently:
//!
//! 1. [`plan`] builds an idempotent Stalwart bulk plan from a mailbox row.
//! 2. [`engine`] applies it with `stalwart-cli` and reads back the account id.
//! 3. [`service`] records that id in PostgreSQL and moves the row to `active`.
//!
//! The one-time password travels from the caller to the engine and is never
//! stored by the control plane. See `docs/DEEP_MAP.ar.md` §15 for the decisions
//! behind this design.

pub mod engine;
pub mod plan;
pub mod service;

pub use engine::{
    ApplyReport, EngineError, EngineSnapshot, MailEngine, StalwartCli, StalwartConfig,
};
pub use plan::{account_upsert, domain_upsert, to_ndjson, PlanLine};
pub use service::{
    Drift, ProvisionError, ProvisionOutcome, Provisioner, ReconcileEntry, ReconcileReport,
};

pub const NAME: &str = "tira-provision";
