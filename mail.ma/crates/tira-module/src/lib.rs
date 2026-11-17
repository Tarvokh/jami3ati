//! Extension points for Tira Mail.
//!
//! A university (UCA, UM5, UIT) must be able to add behaviour — grade
//! notifications, Moodle sync, custom retention — without forking the core.
//!
//! Design note: modules are **in-process** here for the MVP. That is fine for
//! trusted first-party modules, but third-party modules must eventually run
//! out-of-process (a crash or a panic in a plugin must never take down SMTP).
//! See `docs/CORRECTIONS.md`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEvent {
    pub mailbox: String,
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub subject: Option<String>,
    /// Truncated plain-text body; modules must not assume it is complete.
    pub body_preview: String,
}

/// What a module wants to happen to a message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookOutcome {
    pub module: String,
    /// Labels to attach, e.g. `["university/grade"]`.
    pub labels: Vec<String>,
    /// Human-readable note for the audit log.
    pub note: Option<String>,
}

pub trait TiraModule: Send + Sync {
    fn name(&self) -> &str;

    /// Called for every delivered message. Must be cheap and non-blocking;
    /// slow work belongs on a queue, not in the delivery path.
    fn on_message(&self, _event: &MessageEvent) -> HookOutcome {
        HookOutcome {
            module: self.name().to_string(),
            ..HookOutcome::default()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("a module named `{0}` is already registered")]
    Duplicate(String),
    #[error("module names must not be empty")]
    EmptyName,
}

/// Thread-safe module registry. Cheap to clone; clones share the same map.
#[derive(Clone, Default)]
pub struct Registry {
    modules: Arc<RwLock<BTreeMap<String, Arc<dyn TiraModule>>>>,
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("modules", &self.names())
            .finish()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, module: Arc<dyn TiraModule>) -> Result<(), RegistryError> {
        let name = module.name().trim().to_string();
        if name.is_empty() {
            return Err(RegistryError::EmptyName);
        }

        let mut modules = self.modules.write().unwrap_or_else(|e| e.into_inner());
        if modules.contains_key(&name) {
            return Err(RegistryError::Duplicate(name));
        }
        modules.insert(name, module);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn TiraModule>> {
        self.modules
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.modules
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.modules.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Runs every module in name order. A panicking module is isolated so one
    /// bad extension cannot lose a message.
    pub fn dispatch(&self, event: &MessageEvent) -> Vec<HookOutcome> {
        let modules: Vec<Arc<dyn TiraModule>> = self
            .modules
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();

        modules
            .into_iter()
            .filter_map(|module| {
                let name = module.name().to_string();
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| module.on_message(event)))
                    .ok()
                    .or_else(|| {
                        Some(HookOutcome {
                            module: name,
                            labels: vec![],
                            note: Some("module panicked and was isolated".to_string()),
                        })
                    })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tagger {
        name: &'static str,
        label: &'static str,
    }

    impl TiraModule for Tagger {
        fn name(&self) -> &str {
            self.name
        }

        fn on_message(&self, _event: &MessageEvent) -> HookOutcome {
            HookOutcome {
                module: self.name.to_string(),
                labels: vec![self.label.to_string()],
                note: None,
            }
        }
    }

    struct Panicker;

    impl TiraModule for Panicker {
        fn name(&self) -> &str {
            "panicker"
        }

        fn on_message(&self, _event: &MessageEvent) -> HookOutcome {
            panic!("boom");
        }
    }

    fn event() -> MessageEvent {
        MessageEvent {
            mailbox: "yassine@tira.ma".to_string(),
            from: vec!["aicha@uca.ma".to_string()],
            to: vec!["yassine@tira.ma".to_string()],
            subject: Some("Notes S3".to_string()),
            body_preview: "Votre note est disponible".to_string(),
        }
    }

    #[test]
    fn registers_and_dispatches_in_order() {
        let registry = Registry::new();
        registry
            .register(Arc::new(Tagger {
                name: "grades",
                label: "university/grade",
            }))
            .unwrap();
        registry
            .register(Arc::new(Tagger {
                name: "moodle",
                label: "university/moodle",
            }))
            .unwrap();

        let outcomes = registry.dispatch(&event());
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].module, "grades");
        assert_eq!(outcomes[1].module, "moodle");
        assert_eq!(outcomes[0].labels, vec!["university/grade"]);
    }

    #[test]
    fn rejects_duplicates_and_empty_names() {
        let registry = Registry::new();
        registry
            .register(Arc::new(Tagger {
                name: "grades",
                label: "a",
            }))
            .unwrap();
        assert!(matches!(
            registry.register(Arc::new(Tagger {
                name: "grades",
                label: "b"
            })),
            Err(RegistryError::Duplicate(_))
        ));
        assert!(matches!(
            registry.register(Arc::new(Tagger {
                name: "  ",
                label: "c"
            })),
            Err(RegistryError::EmptyName)
        ));
    }

    #[test]
    fn isolates_panicking_modules() {
        let registry = Registry::new();
        registry.register(Arc::new(Panicker)).unwrap();
        registry
            .register(Arc::new(Tagger {
                name: "grades",
                label: "ok",
            }))
            .unwrap();

        let outcomes = registry.dispatch(&event());
        assert_eq!(outcomes.len(), 2, "a panicking module must not lose others");

        let grades = outcomes
            .iter()
            .find(|o| o.module == "grades")
            .expect("healthy module still ran");
        assert_eq!(grades.labels, vec!["ok"]);
        assert!(grades.note.is_none());

        let panicker = outcomes
            .iter()
            .find(|o| o.module == "panicker")
            .expect("panicking module is reported, not dropped");
        assert!(panicker.note.as_deref().unwrap_or("").contains("panicked"));
    }
}
