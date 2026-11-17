//! Workspace source-hygiene guard.
//!
//! `crates/*/src/` must contain only Rust sources (`.rs`). Scratch dumps,
//! captured grep output, and editor backups are not source: they bloat the
//! tree, can be packaged by accident, and obscure what is load-bearing.
//! Legitimate non-`.rs` assets live OUTSIDE `src/` (for example
//! `tira-frontend/assets/` and `tira-store/migrations/`) and are pulled in
//! explicitly via `include_str!` / `include_bytes!`.
//!
//! This guard walks every crate's `src/` at test time and fails on any stray
//! file, so debris is caught by `cargo test --workspace` instead of by a human
//! noticing it later. It auto-covers new crates because it reads the `crates/`
//! directory rather than hard-coding a list. Stray files are quarantined under
//! `quarantine/` (kept for provenance, never deleted), not left in `src/`.

use std::path::{Path, PathBuf};

/// Collects every non-`.rs` file under any `crates/*/src/`, sorted for a
/// deterministic failure message.
fn stray_source_files(crates_dir: &Path) -> Vec<PathBuf> {
    let mut stray = Vec::new();
    let entries =
        std::fs::read_dir(crates_dir).expect("crates dir must be readable for the hygiene guard");
    for entry in entries {
        let entry = entry.expect("read crate dir entry");
        let src = entry.path().join("src");
        if src.is_dir() {
            walk(&src, &mut stray);
        }
    }
    stray.sort();
    stray
}

fn walk(dir: &Path, stray: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("src dir must be readable for the hygiene guard");
    for entry in entries {
        let entry = entry.expect("read src dir entry");
        let path = entry.path();
        if path.is_dir() {
            walk(&path, stray);
        } else if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            stray.push(path);
        }
    }
}

#[test]
fn crate_src_dirs_contain_only_rust_sources() {
    // CARGO_MANIFEST_DIR = <workspace>/crates/tira-store ; the root is two
    // levels up. This is absolute, so the guard is independent of the cwd.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(|crates| crates.parent())
        .expect("workspace root above crates/tira-store");
    let stray = stray_source_files(&root.join("crates"));
    assert!(
        stray.is_empty(),
        "stray non-source files under crates/*/src/ — quarantine them under \
         quarantine/ (never leave scratch in src/):\n{}",
        stray
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
