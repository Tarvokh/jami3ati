//! Content-addressed blob storage.
//!
//! Message bodies and attachments are stored by their SHA-256, laid out as
//! `root/aa/bb/<full-hex>`. Three consequences, all of them intentional:
//!
//! 1. **Deduplication is free.** The same 5 MiB attachment sent to 500
//!    mailboxes is stored once, not 500 times.
//! 2. **Writes are idempotent.** Delivering the same message twice writes the
//!    same path, so a retry after a crash cannot corrupt anything.
//! 3. **Corruption is detectable.** [`BlobStore::get`] re-hashes on read, so
//!    silent bit-rot on disk surfaces as an error instead of as garbage in a
//!    user's mailbox.
//!
//! The trait exists so an S3/MinIO backend can replace the filesystem without
//! touching the metadata layer. The filesystem backend is what a single-node
//! pilot actually needs; see `docs/DEEP_MAP.ar.md` §12.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

/// Reference to a stored blob. `key` is what goes into PostgreSQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef {
    /// Sharded path, e.g. `9f/86/9f86d081...`.
    pub key: String,
    /// Full lowercase hex digest.
    pub sha256_hex: String,
    pub size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("blob not found: {0}")]
    NotFound(String),
    #[error("blob is corrupted (hash mismatch): {0}")]
    Corrupted(String),
    #[error("invalid blob key: {0}")]
    InvalidKey(String),
}

pub trait BlobStore: Send + Sync {
    /// Writes `bytes` and returns their address. Calling it twice with the
    /// same content is a no-op the second time — except that a successful
    /// re-`put` refreshes the blob's last-modified time, which is how the
    /// orphan sweep tells "in use right now" from "abandoned long ago".
    fn put(&self, bytes: &[u8]) -> Result<BlobRef, BlobError>;

    /// Reads a blob and verifies its hash before returning it.
    fn get(&self, key: &str) -> Result<Vec<u8>, BlobError>;

    fn exists(&self, key: &str) -> bool;

    /// Deletes a blob. Returns `false` when it was already gone, so a sweep is
    /// idempotent and two concurrent sweeps cannot error on each other.
    fn remove(&self, key: &str) -> Result<bool, BlobError>;

    /// Deletes a blob only if its last-modified time is still strictly before
    /// `cutoff`, re-checking immediately before the unlink. The orphan sweep
    /// uses this rather than [`BlobStore::remove`] so that a delivery which
    /// deduplicated onto the blob (and refreshed its mtime) after the
    /// candidate list was built is not deleted out from under an open
    /// transaction. Returns `false` when the blob is gone or was touched
    /// recently. The grace window must exceed the longest delivery
    /// transaction for this check to be meaningful.
    fn remove_if_modified_before(&self, key: &str, cutoff: SystemTime) -> Result<bool, BlobError>;

    /// Lists content-addressed keys whose last-modified time is strictly
    /// before `cutoff`. The orphan sweep crosses this with the set of keys
    /// still referenced by `messages`: a key in neither is garbage. Because
    /// `put` refreshes mtime on every delivery (even a deduplicated one), a
    /// blob touched inside the grace window is never a candidate, so an
    /// in-flight transaction cannot have its bytes swept out from under it.
    fn keys_modified_before(&self, cutoff: SystemTime) -> Result<Vec<String>, BlobError>;
}

/// Filesystem backend. Atomic on the same volume: writes go to `tmp/` and are
/// renamed into place, so a crash mid-write never leaves a half-file at a
/// valid key.
#[derive(Debug, Clone)]
pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, BlobError> {
        let root = root.into();
        std::fs::create_dir_all(root.join("tmp"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Maps a content address `aa/bb/<64-hex>` to a path under the root.
    ///
    /// The key must be a well-formed content address ([`is_content_key`]):
    /// three shards, a 64-character lowercase hex digest, and shard prefixes
    /// that match it. That rejects path traversal (`..`, absolute paths,
    /// backslashes, empty segments) *and* any non-content key, so a hostile or
    /// corrupted `blob_key` reaching this layer can never address a file
    /// outside the shard layout. Every real key is produced by [`key_for`], so
    /// the hex constraint is pure defense-in-depth on top of the traversal
    /// guard the sweep already relies on.
    fn path_for(&self, key: &str) -> Result<PathBuf, BlobError> {
        if !is_content_key(key) {
            return Err(BlobError::InvalidKey(key.to_string()));
        }
        let mut path = self.root.clone();
        // `is_content_key` guarantees exactly three clean segments.
        for part in key.split('/') {
            path.push(part);
        }
        Ok(path)
    }
}

fn key_for(hex_digest: &str) -> String {
    format!("{}/{}/{}", &hex_digest[0..2], &hex_digest[2..4], hex_digest)
}

/// True only for a well-formed content address: three shards, a 64-character
/// lowercase hex digest, and the two directory shards matching its prefix.
/// The sweep uses this to ignore stray files (a `.tmp` left by a crash, an
/// operator's note) rather than treating them as collectable blobs.
fn is_content_key(key: &str) -> bool {
    let mut parts = key.split('/');
    let (Some(aa), Some(bb), Some(hex), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && aa == &hex[0..2]
        && bb == &hex[2..4]
}

/// Refreshes a blob's last-modified time to now. Best-effort: the bytes are
/// already valid, so a filesystem that refuses the touch (read-only mount,
/// exotic permissions) must not fail the delivery. The sweep's grace window
/// still protects the blob even if the touch is lost.
fn touch(path: &Path) {
    let result = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
    if let Err(err) = result {
        tracing::warn!(%err, path = %path.display(), "could not refresh blob mtime");
    }
}

impl BlobStore for FsBlobStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobRef, BlobError> {
        let sha256_hex = hex::encode(Sha256::digest(bytes));
        let key = key_for(&sha256_hex);
        let path = self.path_for(&key)?;

        // If a blob already lives at this address, verify it before trusting
        // it. A silent bit-rot, or a zero-length file left by an old crash
        // between write and rename, must not be handed back as "already
        // stored": that would commit metadata pointing at garbage. On a
        // mismatch we fall through and rewrite the correct bytes.
        if path.exists() {
            match std::fs::read(&path) {
                Ok(existing) if hex::encode(Sha256::digest(&existing)) == sha256_hex => {
                    // Valid. Refresh mtime so the orphan sweep treats this
                    // blob as in-use: a deduplicated delivery that is still
                    // mid-commit has just touched it, so a concurrent sweep
                    // cannot select it for deletion.
                    touch(&path);
                    return Ok(BlobRef {
                        key,
                        sha256_hex,
                        size: bytes.len() as u64,
                    });
                }
                Ok(_) => {
                    tracing::warn!(%key, "blob failed integrity check; rewriting");
                }
                // Raced with a removal between exists() and read(): fall
                // through and write it again.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(BlobError::Io(err)),
            }
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to a unique temp file, fsync it, then rename. `rename` is
        // atomic on the same filesystem, so readers never observe a partial
        // blob; the fsync makes the bytes durable *before* the address becomes
        // visible, so a power loss after rename cannot leave a valid key
        // pointing at an empty file.
        let tmp = self
            .root
            .join("tmp")
            .join(format!("{sha256_hex}.{}.tmp", uuid::Uuid::new_v4()));
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        if let Err(err) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(BlobError::Io(err));
        }

        // Make the rename itself durable by syncing the parent directory.
        // Best-effort and platform-dependent: opening a directory as a file
        // fails on Windows, where the move is already journaled by NTFS.
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }

        Ok(BlobRef {
            key,
            sha256_hex,
            size: bytes.len() as u64,
        })
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, BlobError> {
        let path = self.path_for(key)?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(BlobError::NotFound(key.to_string()));
            }
            Err(err) => return Err(BlobError::Io(err)),
        };

        let expected = key.rsplit('/').next().unwrap_or_default();
        let actual = hex::encode(Sha256::digest(&bytes));
        if actual != expected {
            return Err(BlobError::Corrupted(key.to_string()));
        }
        Ok(bytes)
    }

    fn exists(&self, key: &str) -> bool {
        self.path_for(key).map(|p| p.exists()).unwrap_or(false)
    }

    fn remove(&self, key: &str) -> Result<bool, BlobError> {
        let path = self.path_for(key)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(BlobError::Io(err)),
        }
    }

    fn remove_if_modified_before(&self, key: &str, cutoff: SystemTime) -> Result<bool, BlobError> {
        let path = self.path_for(key)?;
        let meta = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(BlobError::Io(err)),
        };
        // A blob touched at or after the cutoff is in use (a delivery just
        // deduplicated onto it); leave it for a later sweep.
        match meta.modified() {
            Ok(modified) if modified >= cutoff => return Ok(false),
            Ok(_) => {}
            // No mtime available: cannot prove it is old, so do not delete.
            Err(_) => return Ok(false),
        }
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(BlobError::Io(err)),
        }
    }

    fn keys_modified_before(&self, cutoff: SystemTime) -> Result<Vec<String>, BlobError> {
        let mut out = Vec::new();
        // Walk the three shard levels (root/aa/bb/<hex>), skipping `tmp/`.
        // Reconstruct each key from its path components and keep only
        // well-formed content addresses whose mtime predates the cutoff.
        let Ok(level1) = std::fs::read_dir(&self.root) else {
            return Ok(out);
        };
        for entry1 in level1.flatten() {
            let p1 = entry1.path();
            if !p1.is_dir() || p1.file_name().is_some_and(|n| n == "tmp") {
                continue;
            }
            let Ok(level2) = std::fs::read_dir(&p1) else {
                continue;
            };
            for entry2 in level2.flatten() {
                let p2 = entry2.path();
                if !p2.is_dir() {
                    continue;
                }
                let Ok(level3) = std::fs::read_dir(&p2) else {
                    continue;
                };
                for entry3 in level3.flatten() {
                    let p3 = entry3.path();
                    if !p3.is_file() {
                        continue;
                    }
                    let (Some(aa), Some(bb), Some(hex)) = (
                        p1.file_name().and_then(|n| n.to_str()),
                        p2.file_name().and_then(|n| n.to_str()),
                        p3.file_name().and_then(|n| n.to_str()),
                    ) else {
                        continue;
                    };
                    let key = format!("{aa}/{bb}/{hex}");
                    if !is_content_key(&key) {
                        continue;
                    }
                    let Ok(meta) = std::fs::metadata(&p3) else {
                        continue;
                    };
                    let Ok(modified) = meta.modified() else {
                        continue;
                    };
                    if modified < cutoff {
                        out.push(key);
                    }
                }
            }
        }
        Ok(out)
    }
}
