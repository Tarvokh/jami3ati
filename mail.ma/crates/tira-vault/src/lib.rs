//! Tira Mail vault: authenticated encryption at rest.
//!
//! Two formats live here, for two different jobs.
//!
//! **Single value — `TIRAV1`** (used for secrets such as a DKIM private key):
//!
//! ```text
//!   "TIRAV1" | salt[16] | nonce[12] | ciphertext+tag
//! ```
//!
//! `seal`/`open` are convenient, but the whole plaintext must fit in memory.
//!
//! **Stream — `TIRAS2`** (used for backups, which can be tens of gigabytes):
//!
//! ```text
//!   "TIRAS2" | salt[16] | chunk_size:u32le
//!   repeated: frame_len:u32le | nonce[12] | flags:u8 | cumulative_total:u64le | ciphertext+tag
//!   last frame: flags=1 (final marker), empty plaintext, cumulative_total = grand total
//! ```
//!
//! The key is derived **once per file** with Argon2id(passphrase, salt).
//! Each frame has a fresh random nonce and authenticates its frame index, a
//! final flag, and the cumulative plaintext length as associated data, so
//! reordering or splicing frames fails to decrypt. The empty final marker
//! authenticates the grand total and must be followed by EOF: a frame-aligned
//! truncation (dropping the last frame or the marker) is now **detectable** —
//! the gap TIRAS1 could only cover with an external plaintext SHA-256.
//! `scripts/restore-drill.ps1` still compares that SHA-256 as defense in depth.
//!
//! **Legacy — `TIRAS1`**: still READ by [`open_stream`] so old archives open,
//! but never written. Its frames authenticate only the index, so it cannot
//! detect frame-aligned truncation; rely on the external SHA-256 for those.
//!
//! This is *at-rest* encryption with a passphrase. It is not end-to-end
//! encryption between users, and it does not replace per-tenant key
//! management (KMS). See `docs/CORRECTIONS.md`.

use std::io::{self, Read, Write};

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::rngs::OsRng;
use rand::RngCore;

const MAGIC: &[u8; 6] = b"TIRAV1";
/// Legacy stream magic. Still READ by [`open_stream`] so old archives keep
/// working; new seals never write it (see `STREAM_MAGIC_V2`).
const STREAM_MAGIC: &[u8; 6] = b"TIRAS1";
/// Current stream magic. Adds an authenticated final-frame flag and a
/// cumulative plaintext length, so frame-aligned truncation is detectable
/// (the gap TIRAS1 had to cover with an external SHA-256).
const STREAM_MAGIC_V2: &[u8; 6] = b"TIRAS2";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = MAGIC.len() + SALT_LEN + NONCE_LEN;
const STREAM_HEADER_LEN: usize = STREAM_MAGIC.len() + SALT_LEN + 4;
/// TIRAS1 frame: `nonce[12] | ciphertext+tag`.
const FRAME_OVERHEAD: usize = NONCE_LEN + TAG_LEN;
/// TIRAS2 frame header adds a final flag (1 byte) and the cumulative plaintext
/// length (8 bytes) after the nonce. Both are authenticated through the AAD.
const FLAGS_LEN: usize = 1;
const TOTAL_LEN: usize = 8;
/// TIRAS2 frame: `nonce[12] | flags[1] | cumulative_total[8] | ciphertext+tag`.
const FRAME_OVERHEAD_V2: usize = NONCE_LEN + FLAGS_LEN + TOTAL_LEN + TAG_LEN;
/// TIRAS2 associated data: `frame_index(u64le) | flags(u8) | cumulative(u64le)`.
const AAD_LEN_V2: usize = 8 + FLAGS_LEN + TOTAL_LEN;
/// A data frame (more frames may follow).
const FLAG_DATA: u8 = 0;
/// The last frame: authenticates the grand total and must be followed by EOF.
const FLAG_FINAL: u8 = 1;

/// Default frame size for [`seal_stream`]: 1 MiB.
pub const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;

/// Upper bound on a single frame. Big enough that the CLI can use 64 MiB
/// chunks, small enough that a hostile header cannot make us allocate wildly.
pub const MAX_CHUNK_SIZE: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("key derivation failed")]
    Kdf,
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed: wrong passphrase or corrupted data")]
    Decrypt,
    #[error("blob is truncated or not a Tira vault blob")]
    Malformed,
    #[error("chunk size must be between 1 and {max} bytes, got {got}")]
    InvalidChunkSize { got: usize, max: usize },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// Encrypts `plaintext` under `passphrase`, returning a self-contained blob.
pub fn seal(plaintext: &[u8], passphrase: &[u8]) -> Result<Vec<u8>, VaultError> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);

    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| VaultError::Encrypt)?;

    let mut blob = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    blob.extend_from_slice(MAGIC);
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// Decrypts a blob produced by [`seal`].
pub fn open(blob: &[u8], passphrase: &[u8]) -> Result<Vec<u8>, VaultError> {
    if blob.len() < HEADER_LEN + TAG_LEN {
        return Err(VaultError::Malformed);
    }
    if &blob[..MAGIC.len()] != MAGIC {
        return Err(VaultError::Malformed);
    }

    let salt = &blob[MAGIC.len()..MAGIC.len() + SALT_LEN];
    let nonce = &blob[MAGIC.len() + SALT_LEN..HEADER_LEN];
    let ciphertext = &blob[HEADER_LEN..];

    let key = derive_key(passphrase, salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));

    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| VaultError::Decrypt)
}

/// Encrypts a stream to `output` in the **TIRAS2** format, returning the number
/// of plaintext bytes read.
///
/// Memory use is `chunk_size` plus one frame, regardless of input size. The
/// derived key is deliberately **not** stretched per frame: Argon2id costs
/// ~50 ms, which is fine once per file and unacceptable once per megabyte.
///
/// Every frame authenticates its index, a final flag, and the cumulative
/// plaintext length through the AAD. After the data frames, one empty final
/// frame (`FLAG_FINAL`) authenticates the grand total. A reader therefore
/// detects frame-aligned truncation: dropping the last data frame leaves no
/// final marker, and dropping the marker itself fails the same check.
pub fn seal_stream<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    passphrase: &[u8],
    chunk_size: usize,
) -> Result<u64, VaultError> {
    validate_chunk_size(chunk_size)?;

    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));

    output.write_all(STREAM_MAGIC_V2)?;
    output.write_all(&salt)?;
    output.write_all(&(chunk_size as u32).to_le_bytes())?;

    let mut buffer = vec![0u8; chunk_size];
    let mut frame_index: u64 = 0;
    let mut total: u64 = 0;

    loop {
        let read = read_fill(&mut input, &mut buffer)?;
        // EOF: no more data frames. An empty input emits zero data frames and
        // is still terminated by the authenticated final marker below, so the
        // file is never just a header a reader could mistake for a truncation.
        if read == 0 {
            break;
        }
        total += read as u64;
        emit_frame_v2(
            &cipher,
            &mut output,
            frame_index,
            FLAG_DATA,
            total,
            &buffer[..read],
        )?;
        frame_index += 1;

        // A short read means end of input; stop rather than read again.
        if read < chunk_size {
            break;
        }
    }

    // Final marker: empty plaintext, authenticates the grand total. The reader
    // requires this frame and then EOF, which is what makes truncation fatal.
    emit_frame_v2(&cipher, &mut output, frame_index, FLAG_FINAL, total, &[])?;

    Ok(total)
}

/// Builds the TIRAS2 associated data: `index(u64le) | flags(u8) | total(u64le)`.
/// Shared by sealing and opening so the two can never drift apart.
fn frame_aad_v2(index: u64, flags: u8, cumulative: u64) -> [u8; AAD_LEN_V2] {
    let mut aad = [0u8; AAD_LEN_V2];
    aad[..8].copy_from_slice(&index.to_le_bytes());
    aad[8] = flags;
    aad[9..].copy_from_slice(&cumulative.to_le_bytes());
    aad
}

/// Encrypts and writes one TIRAS2 frame:
/// `frame_len(u32le) | nonce[12] | flags[1] | cumulative_total[8] | ciphertext+tag`.
/// The flag and cumulative length sit in the clear but are bound into the AAD,
/// so flipping either byte breaks the tag.
fn emit_frame_v2<W: Write>(
    cipher: &ChaCha20Poly1305,
    output: &mut W,
    index: u64,
    flags: u8,
    cumulative: u64,
    plaintext: &[u8],
) -> Result<(), VaultError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);

    let aad = frame_aad_v2(index, flags, cumulative);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::Encrypt)?;

    let frame_len = NONCE_LEN + FLAGS_LEN + TOTAL_LEN + ciphertext.len();
    output.write_all(&(frame_len as u32).to_le_bytes())?;
    output.write_all(&nonce_bytes)?;
    output.write_all(&[flags])?;
    output.write_all(&cumulative.to_le_bytes())?;
    output.write_all(&ciphertext)?;
    Ok(())
}

/// Decrypts a stream produced by [`seal_stream`], returning plaintext bytes.
///
/// Reads both the current **TIRAS2** format and the legacy **TIRAS1** format,
/// so archives sealed before the upgrade still open. New seals are always
/// TIRAS2. TIRAS1 cannot detect frame-aligned truncation (it has no final
/// marker); TIRAS2 can.
pub fn open_stream<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    passphrase: &[u8],
) -> Result<u64, VaultError> {
    let mut header = [0u8; STREAM_HEADER_LEN];
    read_exact_malformed(&mut input, &mut header)?;

    let magic = &header[..STREAM_MAGIC.len()];
    let v2 = if magic == STREAM_MAGIC_V2 {
        true
    } else if magic == STREAM_MAGIC {
        false
    } else {
        return Err(VaultError::Malformed);
    };

    let salt = &header[STREAM_MAGIC.len()..STREAM_MAGIC.len() + SALT_LEN];
    let chunk_size = u32::from_le_bytes(
        header[STREAM_MAGIC.len() + SALT_LEN..]
            .try_into()
            .expect("header slice is exactly four bytes"),
    ) as usize;
    validate_chunk_size(chunk_size)?;

    let key = derive_key(passphrase, salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));

    if v2 {
        open_stream_v2(&mut input, &mut output, &cipher, chunk_size)
    } else {
        open_stream_v1(&mut input, &mut output, &cipher, chunk_size)
    }
}

/// Legacy TIRAS1 reader: each frame authenticates only its index. A
/// frame-aligned truncation is NOT detectable here (no final marker); callers
/// relying on TIRAS1 must keep comparing an external plaintext SHA-256.
fn open_stream_v1<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    cipher: &ChaCha20Poly1305,
    chunk_size: usize,
) -> Result<u64, VaultError> {
    let mut frame_index: u64 = 0;
    let mut total: u64 = 0;

    while let Some(frame_len) = read_frame_len(input)? {
        let frame_len = frame_len as usize;
        if !(FRAME_OVERHEAD..=FRAME_OVERHEAD + chunk_size).contains(&frame_len) {
            return Err(VaultError::Malformed);
        }

        let mut frame = vec![0u8; frame_len];
        read_exact_malformed(input, &mut frame)?;

        let nonce = &frame[..NONCE_LEN];
        let ciphertext = &frame[NONCE_LEN..];

        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &frame_index.to_le_bytes(),
                },
            )
            .map_err(|_| VaultError::Decrypt)?;

        output.write_all(&plaintext)?;
        frame_index += 1;
        total += plaintext.len() as u64;
    }

    Ok(total)
}

/// TIRAS2 reader: each frame authenticates `index | flags | cumulative_total`.
/// The cumulative length is cross-checked against the bytes actually decoded,
/// a final frame must appear, and nothing may follow it. Dropping or appending
/// frames, or truncating the tail, is therefore fatal instead of silent.
fn open_stream_v2<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    cipher: &ChaCha20Poly1305,
    chunk_size: usize,
) -> Result<u64, VaultError> {
    let mut frame_index: u64 = 0;
    let mut total: u64 = 0;
    let mut saw_final = false;

    while let Some(frame_len) = read_frame_len(input)? {
        // Anything after the final marker is a spliced tail: reject.
        if saw_final {
            return Err(VaultError::Malformed);
        }
        let frame_len = frame_len as usize;
        if !(FRAME_OVERHEAD_V2..=FRAME_OVERHEAD_V2 + chunk_size).contains(&frame_len) {
            return Err(VaultError::Malformed);
        }

        let mut frame = vec![0u8; frame_len];
        read_exact_malformed(input, &mut frame)?;

        let nonce = &frame[..NONCE_LEN];
        let flags = frame[NONCE_LEN];
        if flags != FLAG_DATA && flags != FLAG_FINAL {
            return Err(VaultError::Malformed);
        }
        let cumulative = u64::from_le_bytes(
            frame[NONCE_LEN + FLAGS_LEN..NONCE_LEN + FLAGS_LEN + TOTAL_LEN]
                .try_into()
                .expect("slice is exactly eight bytes"),
        );
        let ciphertext = &frame[NONCE_LEN + FLAGS_LEN + TOTAL_LEN..];

        let aad = frame_aad_v2(frame_index, flags, cumulative);
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| VaultError::Decrypt)?;

        // The authenticated cumulative length must equal the bytes decoded so
        // far: defense in depth on top of the tag, and it pins the grand total
        // carried by the final marker.
        if total + plaintext.len() as u64 != cumulative {
            return Err(VaultError::Malformed);
        }

        output.write_all(&plaintext)?;
        total += plaintext.len() as u64;
        frame_index += 1;
        if flags == FLAG_FINAL {
            saw_final = true;
        }
    }

    // EOF without a final marker means the tail was truncated.
    if !saw_final {
        return Err(VaultError::Malformed);
    }

    Ok(total)
}

fn validate_chunk_size(chunk_size: usize) -> Result<(), VaultError> {
    if chunk_size == 0 || chunk_size > MAX_CHUNK_SIZE {
        return Err(VaultError::InvalidChunkSize {
            got: chunk_size,
            max: MAX_CHUNK_SIZE,
        });
    }
    Ok(())
}

fn read_fill<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
    Ok(filled)
}

fn read_exact_malformed<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<(), VaultError> {
    reader.read_exact(buf).map_err(|err| match err.kind() {
        io::ErrorKind::UnexpectedEof => VaultError::Malformed,
        _ => VaultError::Io(err),
    })
}

/// Reads a little-endian `u32` frame length, distinguishing a clean end of
/// stream (`None`) from a header that ends mid-length (`Malformed`).
fn read_frame_len<R: Read>(reader: &mut R) -> Result<Option<u32>, VaultError> {
    let mut len = [0u8; 4];
    loop {
        match reader.read(&mut len[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(VaultError::Io(err)),
        }
    }
    read_exact_malformed(reader, &mut len[1..])?;
    Ok(Some(u32::from_le_bytes(len)))
}

fn derive_key(passphrase: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN], VaultError> {
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|_| VaultError::Kdf)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip() {
        let blob = seal(b"Salam, hada sirr.", b"correct horse battery staple").unwrap();
        let plain = open(&blob, b"correct horse battery staple").unwrap();
        assert_eq!(plain, b"Salam, hada sirr.");
    }

    #[test]
    fn same_plaintext_encrypts_differently() {
        let a = seal(b"same", b"passphrase").unwrap();
        let b = seal(b"same", b"passphrase").unwrap();
        assert_ne!(a, b, "salt and nonce must be random per seal");
    }

    #[test]
    fn wrong_passphrase_fails() {
        let blob = seal(b"secret", b"right").unwrap();
        assert!(matches!(open(&blob, b"wrong"), Err(VaultError::Decrypt)));
    }

    #[test]
    fn tampering_is_detected() {
        let mut blob = seal(b"secret", b"pass").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        assert!(matches!(open(&blob, b"pass"), Err(VaultError::Decrypt)));
    }

    #[test]
    fn truncated_blob_is_rejected() {
        let blob = seal(b"secret", b"pass").unwrap();
        assert!(matches!(
            open(&blob[..10], b"pass"),
            Err(VaultError::Malformed)
        ));
        assert!(matches!(
            open(b"not-a-blob-at-all-yyyyy", b"pass"),
            Err(VaultError::Malformed)
        ));
    }

    fn stream_roundtrip(data: &[u8], chunk_size: usize) -> Vec<u8> {
        let mut sealed = Vec::new();
        let written = seal_stream(Cursor::new(data), &mut sealed, b"pass", chunk_size).unwrap();
        assert_eq!(written, data.len() as u64);

        let mut opened = Vec::new();
        let read = open_stream(Cursor::new(&sealed), &mut opened, b"pass").unwrap();
        assert_eq!(read, data.len() as u64);
        opened
    }

    #[test]
    fn stream_roundtrips_across_frames() {
        let data: Vec<u8> = (0..(32 * 3 + 17)).map(|i| (i % 251) as u8).collect();
        assert_eq!(stream_roundtrip(&data, 32), data);
    }

    #[test]
    fn stream_handles_exact_frame_boundaries_and_empty_input() {
        let data: Vec<u8> = (0..64).map(|i| i as u8).collect();
        assert_eq!(stream_roundtrip(&data, 32), data);
        assert_eq!(stream_roundtrip(b"", 32), b"");
    }

    #[test]
    fn stream_rejects_a_wrong_passphrase() {
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"secret"), &mut sealed, b"right", 8).unwrap();
        let mut out = Vec::new();
        assert!(matches!(
            open_stream(Cursor::new(&sealed), &mut out, b"wrong"),
            Err(VaultError::Decrypt)
        ));
    }

    #[test]
    fn stream_detects_reordered_frames() {
        // Two frames of four bytes: each frame is 4 (len) + 12 (nonce) + 4 + 16 (tag).
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"abcdefgh"), &mut sealed, b"pass", 4).unwrap();

        let first_len = u32::from_le_bytes(
            sealed[STREAM_HEADER_LEN..STREAM_HEADER_LEN + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let first_end = STREAM_HEADER_LEN + 4 + first_len;
        let second_len =
            u32::from_le_bytes(sealed[first_end..first_end + 4].try_into().unwrap()) as usize;
        let second_end = first_end + 4 + second_len;

        // Rebuild the stream with the two *whole* frames (length prefix
        // included) in the opposite order. Swapping only the ciphertext would
        // also corrupt the framing, which would test the wrong failure.
        let mut swapped = sealed[..STREAM_HEADER_LEN].to_vec();
        swapped.extend_from_slice(&sealed[first_end..second_end]);
        swapped.extend_from_slice(&sealed[STREAM_HEADER_LEN..first_end]);

        let mut out = Vec::new();
        assert!(matches!(
            open_stream(Cursor::new(&swapped), &mut out, b"pass"),
            Err(VaultError::Decrypt)
        ));
    }

    #[test]
    fn stream_detects_truncation_inside_a_frame() {
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"abcdefgh"), &mut sealed, b"pass", 4).unwrap();
        sealed.truncate(sealed.len() - 1);
        let mut out = Vec::new();
        assert!(open_stream(Cursor::new(&sealed), &mut out, b"pass").is_err());
    }

    #[test]
    fn stream_rejects_impossible_chunk_sizes() {
        assert!(matches!(
            seal_stream(Cursor::new(b"x"), Vec::new(), b"pass", 0),
            Err(VaultError::InvalidChunkSize { .. })
        ));
        assert!(matches!(
            seal_stream(Cursor::new(b"x"), Vec::new(), b"pass", MAX_CHUNK_SIZE + 1),
            Err(VaultError::InvalidChunkSize { .. })
        ));
    }

    #[test]
    fn stream_rejects_a_non_vault_header() {
        let mut out = Vec::new();
        assert!(matches!(
            open_stream(Cursor::new(b"TIRAV1and then some bytes"), &mut out, b"pass"),
            Err(VaultError::Malformed)
        ));
    }

    /// Start offset of every frame's length prefix (works for TIRAS1 and
    /// TIRAS2 alike, since both prefix each frame with a `u32le` length).
    fn frame_offsets(sealed: &[u8]) -> Vec<usize> {
        let mut off = STREAM_HEADER_LEN;
        let mut starts = Vec::new();
        while off + 4 <= sealed.len() {
            let len = u32::from_le_bytes(sealed[off..off + 4].try_into().unwrap()) as usize;
            starts.push(off);
            off += 4 + len;
        }
        starts
    }

    #[test]
    fn stream_v2_writes_tiras2_magic() {
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"payload"), &mut sealed, b"pass", 4).unwrap();
        assert_eq!(&sealed[..STREAM_MAGIC.len()], STREAM_MAGIC_V2);
    }

    #[test]
    fn stream_v2_detects_frame_aligned_truncation() {
        // Two data frames + one final marker. Dropping the marker (a clean,
        // frame-aligned truncation) must fail — this is the gap TIRAS1 had.
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"abcdefgh"), &mut sealed, b"pass", 4).unwrap();
        let offs = frame_offsets(&sealed);
        assert_eq!(offs.len(), 3, "two data frames and a final marker");
        let truncated = &sealed[..offs[2]];
        let mut out = Vec::new();
        assert!(matches!(
            open_stream(Cursor::new(truncated), &mut out, b"pass"),
            Err(VaultError::Malformed)
        ));
    }

    #[test]
    fn stream_v2_detects_trailing_frame_after_final() {
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"abcd"), &mut sealed, b"pass", 4).unwrap();
        let offs = frame_offsets(&sealed);
        let last = *offs.last().unwrap();
        // Splice a copy of the final frame onto the end: nothing may follow it.
        let mut extended = sealed.clone();
        extended.extend_from_slice(&sealed[last..]);
        let mut out = Vec::new();
        assert!(matches!(
            open_stream(Cursor::new(&extended), &mut out, b"pass"),
            Err(VaultError::Malformed)
        ));
    }

    #[test]
    fn stream_v2_detects_final_flag_tamper() {
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"abcdefgh"), &mut sealed, b"pass", 4).unwrap();
        let offs = frame_offsets(&sealed);
        // flags byte sits right after the 4-byte length and 12-byte nonce.
        let flags_at = offs[0] + 4 + NONCE_LEN;
        assert_eq!(sealed[flags_at], FLAG_DATA);
        sealed[flags_at] = FLAG_FINAL;
        let mut out = Vec::new();
        assert!(matches!(
            open_stream(Cursor::new(&sealed), &mut out, b"pass"),
            Err(VaultError::Decrypt)
        ));
    }

    #[test]
    fn stream_v2_detects_total_length_tamper() {
        let mut sealed = Vec::new();
        seal_stream(Cursor::new(b"abcd"), &mut sealed, b"pass", 4).unwrap();
        let offs = frame_offsets(&sealed);
        let last = *offs.last().unwrap();
        // cumulative_total sits after length(4) + nonce(12) + flags(1).
        let total_at = last + 4 + NONCE_LEN + FLAGS_LEN;
        sealed[total_at] ^= 0x01;
        let mut out = Vec::new();
        assert!(matches!(
            open_stream(Cursor::new(&sealed), &mut out, b"pass"),
            Err(VaultError::Decrypt)
        ));
    }

    /// Test-only fixture writer for the legacy TIRAS1 format, so backward
    /// compatibility of the reader is proven without keeping dead production
    /// code. Mirrors the pre-TIRAS2 `seal_stream` byte for byte.
    fn seal_stream_v1<R: Read, W: Write>(
        mut input: R,
        mut output: W,
        passphrase: &[u8],
        chunk_size: usize,
    ) -> u64 {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let key = derive_key(passphrase, &salt).unwrap();
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        output.write_all(STREAM_MAGIC).unwrap();
        output.write_all(&salt).unwrap();
        output
            .write_all(&(chunk_size as u32).to_le_bytes())
            .unwrap();

        let mut buffer = vec![0u8; chunk_size];
        let mut frame_index: u64 = 0;
        let mut total: u64 = 0;
        loop {
            let read = read_fill(&mut input, &mut buffer).unwrap();
            if read == 0 && frame_index > 0 {
                break;
            }
            let mut nonce_bytes = [0u8; NONCE_LEN];
            OsRng.fill_bytes(&mut nonce_bytes);
            let ciphertext = cipher
                .encrypt(
                    Nonce::from_slice(&nonce_bytes),
                    Payload {
                        msg: &buffer[..read],
                        aad: &frame_index.to_le_bytes(),
                    },
                )
                .unwrap();
            let frame_len = NONCE_LEN + ciphertext.len();
            output.write_all(&(frame_len as u32).to_le_bytes()).unwrap();
            output.write_all(&nonce_bytes).unwrap();
            output.write_all(&ciphertext).unwrap();
            frame_index += 1;
            total += read as u64;
            if read < chunk_size {
                break;
            }
        }
        total
    }

    #[test]
    fn stream_v1_legacy_still_opens() {
        let data = b"legacy archive bytes that must still open after upgrade";
        let mut sealed = Vec::new();
        let written = seal_stream_v1(Cursor::new(data), &mut sealed, b"pass", 8);
        assert_eq!(written, data.len() as u64);
        assert_eq!(&sealed[..STREAM_MAGIC.len()], STREAM_MAGIC);
        let mut opened = Vec::new();
        let read = open_stream(Cursor::new(&sealed), &mut opened, b"pass").unwrap();
        assert_eq!(read, data.len() as u64);
        assert_eq!(opened, data);
    }

    #[test]
    fn stream_v1_truncation_is_not_detected_documents_legacy_gap() {
        // Honest documentation of why TIRAS2 exists: a TIRAS1 stream has no
        // final marker, so dropping the last frame still opens (silently short).
        let mut sealed = Vec::new();
        seal_stream_v1(Cursor::new(b"abcdefgh"), &mut sealed, b"pass", 4);
        let offs = frame_offsets(&sealed);
        assert_eq!(offs.len(), 2);
        let truncated = &sealed[..offs[1]];
        let mut opened = Vec::new();
        let read = open_stream(Cursor::new(truncated), &mut opened, b"pass").unwrap();
        assert_eq!(read, 4, "legacy reader returns only the surviving frame");
        assert_eq!(opened, b"abcd");
    }
}
