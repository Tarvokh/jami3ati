use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use tira_store::blob::{BlobError, BlobStore, FsBlobStore};
use uuid::Uuid;

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("tira-blob-test-{}", Uuid::new_v4()))
}

/// Backdates a file's last-modified time so the sweep's grace window can be
/// exercised without waiting in real time.
fn age(path: &std::path::Path, by: Duration) {
    let when = SystemTime::now() - by;
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open to age")
        .set_modified(when)
        .expect("set modified");
}

#[test]
fn put_is_content_addressed_and_deduplicated() {
    let store = FsBlobStore::new(temp_root()).expect("create store");

    let a = store.put(b"Salam, hada test.").expect("put a");
    let b = store.put(b"Salam, hada test.").expect("put b");

    assert_eq!(a, b, "same bytes must produce the same reference");
    assert_eq!(a.size, 17);
    assert_eq!(a.sha256_hex.len(), 64);
    assert_eq!(
        a.key,
        format!(
            "{}/{}/{}",
            &a.sha256_hex[0..2],
            &a.sha256_hex[2..4],
            a.sha256_hex
        )
    );

    // Exactly one file exists on disk for two puts.
    let mut count = 0;
    for entry in walk(store.root()) {
        if entry.extension().map(|e| e == "tmp").unwrap_or(false) {
            continue;
        }
        count += 1;
    }
    assert_eq!(count, 1, "deduplication must store the bytes once");
}

#[test]
fn get_returns_exact_bytes() {
    let store = FsBlobStore::new(temp_root()).expect("create store");
    let payload = b"\x00\x01\x02 binary \xff payload";
    let blob = store.put(payload).expect("put");
    assert_eq!(store.get(&blob.key).expect("get"), payload);
    assert!(store.exists(&blob.key));
}

#[test]
fn corruption_is_detected_on_read() {
    let root = temp_root();
    let store = FsBlobStore::new(&root).expect("create store");
    let blob = store.put(b"important message").expect("put");

    let path = root.join(&blob.key);
    std::fs::write(&path, b"tampered").expect("overwrite blob");

    match store.get(&blob.key) {
        Err(BlobError::Corrupted(key)) => assert_eq!(key, blob.key),
        other => panic!("expected corruption error, got {other:?}"),
    }
}

#[test]
fn missing_blob_reports_not_found() {
    let store = FsBlobStore::new(temp_root()).expect("create store");
    // A well-formed content address (shards match the digest prefix) that was
    // never written: the path layer accepts it, the read finds no file.
    let missing = format!("00/00/{}", "0".repeat(64));
    assert!(matches!(store.get(&missing), Err(BlobError::NotFound(_))));
    assert!(!store.exists(&missing));
}

#[test]
fn path_for_rejects_a_non_content_key_even_without_traversal() {
    let store = FsBlobStore::new(temp_root()).expect("create store");
    // Three clean shards, no `..`, no backslash — yet not a content address.
    // Defense-in-depth: the path layer must reject these rather than map them
    // to a file, so a corrupted `blob_key` can never name an arbitrary path.
    let keys = [
        "aa/bb/not-a-digest".to_string(),
        "zz/zz/zz".to_string(),
        format!("ab/cd/{}", "g".repeat(64)),
        format!("AB/CD/{}", "a".repeat(64)),
        format!("ab/cd/{}", "a".repeat(63)),
        format!("00/00/{}", "0".repeat(65)),
        // Shards that do not match the digest prefix.
        format!("ab/cd/{}", "0".repeat(64)),
    ];
    for key in &keys {
        assert!(
            matches!(store.get(key), Err(BlobError::InvalidKey(_))),
            "key {key:?} must be rejected at the path layer"
        );
        assert!(!store.exists(key), "key {key:?} must not resolve to a path");
    }
}

#[test]
fn hostile_keys_cannot_escape_the_root() {
    let store = FsBlobStore::new(temp_root()).expect("create store");
    for key in [
        "../../etc/passwd",
        "aa/../../secret",
        "aa/bb",
        "/absolute/path",
        "aa\\bb\\cc",
        "",
    ] {
        assert!(
            matches!(store.get(key), Err(BlobError::InvalidKey(_))),
            "key {key:?} must be rejected"
        );
    }
}

#[test]
fn put_repairs_a_corrupted_blob_instead_of_trusting_it() {
    let root = temp_root();
    let store = FsBlobStore::new(&root).expect("create store");
    let payload = b"the real message bytes, not the rot";
    let blob = store.put(payload).expect("put");

    // Simulate bit-rot: the address is intact, the bytes are not.
    std::fs::write(root.join(&blob.key), b"rot").expect("corrupt");
    assert!(matches!(store.get(&blob.key), Err(BlobError::Corrupted(_))));

    // A second put must verify, find the mismatch, and rewrite — not hand the
    // corrupted file back as "already stored".
    let repaired = store.put(payload).expect("repair put");
    assert_eq!(repaired.key, blob.key);
    assert_eq!(store.get(&blob.key).expect("get after repair"), payload);
}

#[test]
fn dedup_put_refreshes_mtime_so_the_sweep_spares_an_in_use_blob() {
    let store = FsBlobStore::new(temp_root()).expect("create store");
    let payload = b"a shared 5 MiB attachment";
    let blob = store.put(payload).expect("put");
    let path = store.root().join(&blob.key);

    // Age it past a one-hour grace window: now it looks like an orphan.
    age(&path, Duration::from_secs(7200));
    let cutoff = SystemTime::now() - Duration::from_secs(3600);
    assert!(
        store
            .keys_modified_before(cutoff)
            .expect("list")
            .contains(&blob.key),
        "an aged blob must be a sweep candidate"
    );

    // A new delivery deduplicates onto it. `put` must refresh mtime so the
    // concurrent sweep no longer considers it garbage.
    store.put(payload).expect("dedup put");
    assert!(
        !store
            .keys_modified_before(cutoff)
            .expect("list")
            .contains(&blob.key),
        "a re-put blob must be spared by the grace window"
    );
}

#[test]
fn remove_if_modified_before_declines_a_recently_touched_blob() {
    let store = FsBlobStore::new(temp_root()).expect("create store");
    let blob = store.put(b"live bytes").expect("put");
    let path = store.root().join(&blob.key);
    let cutoff = SystemTime::now() - Duration::from_secs(3600);

    // mtime is now: newer than the cutoff, so the conditional delete declines.
    assert!(!store
        .remove_if_modified_before(&blob.key, cutoff)
        .expect("conditional remove"));
    assert!(store.exists(&blob.key), "a recent blob must survive");

    // Age it past the cutoff: now the delete proceeds.
    age(&path, Duration::from_secs(7200));
    assert!(store
        .remove_if_modified_before(&blob.key, cutoff)
        .expect("remove old"));
    assert!(!store.exists(&blob.key));

    // Idempotent: a second sweep does not error on the missing blob.
    assert!(!store
        .remove_if_modified_before(&blob.key, cutoff)
        .expect("remove again"));
}

#[test]
fn remove_is_idempotent() {
    let store = FsBlobStore::new(temp_root()).expect("create store");
    let blob = store.put(b"x").expect("put");
    assert!(store.remove(&blob.key).expect("first remove"));
    assert!(!store.remove(&blob.key).expect("second remove"));
    assert!(!store.exists(&blob.key));
}

#[test]
fn sweep_lists_only_well_formed_old_blobs_and_ignores_stray_files() {
    let root = temp_root();
    let store = FsBlobStore::new(&root).expect("create store");
    let blob = store.put(b"a real blob").expect("put");
    let path = root.join(&blob.key);
    age(&path, Duration::from_secs(7200));

    // A stray note in the same shard directory, aged just as old. Only the
    // content-address shape check excludes it from the sweep.
    let shard = path.parent().expect("shard dir").to_path_buf();
    let stray = shard.join("not-a-blob.txt");
    std::fs::write(&stray, b"operator note").expect("stray");
    age(&stray, Duration::from_secs(7200));
    // A stray file at the root, and the always-present tmp directory.
    std::fs::write(root.join("README"), b"junk").expect("root stray");
    std::fs::write(root.join("tmp").join("leftover.tmp"), b"junk").expect("tmp stray");

    let cutoff = SystemTime::now() - Duration::from_secs(3600);
    let keys = store.keys_modified_before(cutoff).expect("list");
    assert_eq!(
        keys,
        vec![blob.key.clone()],
        "only the real aged blob is a candidate"
    );
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
