//! `tira-seal` — stream-encrypt and decrypt Tira backup files.
//!
//! The passphrase is read from the environment, never from argv: on Linux and
//! Windows any local user can read another process's command line, and a
//! passphrase in shell history is worse.
//!
//! ```text
//! $env:TIRA_SEAL_PASSPHRASE = "..."
//! tira-seal seal   --in db.dump   --out db.dump.tira --chunk-mib 4
//! tira-seal verify --in db.dump.tira
//! tira-seal open   --in db.dump.tira --out db.dump
//! ```
//!
//! `-` means stdin/stdout, so a backup can be piped without a temporary file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rand::rngs::OsRng;
use rand::RngCore;
use tira_vault::{open_stream, seal_stream, VaultError, MAX_CHUNK_SIZE};

const PASSPHRASE_ENV: &str = "TIRA_SEAL_PASSPHRASE";

const USAGE: &str = "\
tira-seal — stream-encrypt Tira backups (ChaCha20-Poly1305 + Argon2id)

USAGE:
  tira-seal seal   --in <path|-> --out <path|-> [--chunk-mib <n>] [--force]
  tira-seal open   --in <path|-> --out <path|-> [--force]
  tira-seal verify --in <path|->

ENVIRONMENT:
  TIRA_SEAL_PASSPHRASE   required; never passed on the command line

EXIT CODES:
  0 success, 1 cryptographic or io failure, 2 usage error
";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Seal,
    Open,
    Verify,
}

struct Args {
    command: Command,
    input: String,
    output: Option<String>,
    chunk_mib: usize,
    force: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let command = match it.next().as_deref() {
        Some("seal") => Command::Seal,
        Some("open") => Command::Open,
        Some("verify") => Command::Verify,
        Some("-h") | Some("--help") => return Err(String::new()),
        Some(other) => return Err(format!("unknown command: {other}")),
        None => return Err("missing command".to_string()),
    };

    let mut input = None;
    let mut output = None;
    let mut chunk_mib = 4usize;
    let mut force = false;

    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--in" => {
                input = Some(
                    it.next()
                        .ok_or_else(|| "--in needs a path or -".to_string())?,
                );
            }
            "--out" => {
                output = Some(
                    it.next()
                        .ok_or_else(|| "--out needs a path or -".to_string())?,
                );
            }
            "--chunk-mib" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--chunk-mib needs a number".to_string())?;
                let mib: usize = raw
                    .parse()
                    .map_err(|_| format!("--chunk-mib must be an integer, got {raw}"))?;
                if mib == 0 {
                    return Err("--chunk-mib must be at least 1".to_string());
                }
                chunk_mib = mib;
            }
            "--force" => force = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let input = input.ok_or_else(|| "--in is required".to_string())?;
    if command != Command::Verify && output.is_none() {
        return Err("--out is required".to_string());
    }

    Ok(Args {
        command,
        input,
        output,
        chunk_mib,
        force,
    })
}

fn passphrase() -> Result<Vec<u8>, String> {
    match std::env::var(PASSPHRASE_ENV) {
        Ok(value) if !value.is_empty() => Ok(value.into_bytes()),
        Ok(_) => Err(format!("{PASSPHRASE_ENV} is empty")),
        Err(_) => Err(format!("{PASSPHRASE_ENV} is not set")),
    }
}

fn open_input(path: &str) -> io::Result<Box<dyn Read>> {
    if path == "-" {
        Ok(Box::new(io::stdin()))
    } else {
        Ok(Box::new(BufReader::new(File::open(path)?)))
    }
}

/// Where command output goes: stdout (no rename) or an atomic file.
///
/// M-17: a file is written to a private temp sibling (0600 on Unix) and renamed
/// over the target only after the whole stream succeeds. This fixes three
/// leaks in the old `File::create`:
///   * the decrypted plaintext (`open`) was world-readable (0644);
///   * `--force` truncated an existing target *before* authentication, so a
///     wrong passphrase destroyed the previous file;
///   * a mid-stream failure left a partial file at the final path.
///
/// Now the target is untouched until success, and the temp is removed on any
/// error (see the `Drop` guard).
enum Output {
    Stdout(BufWriter<io::Stdout>),
    File(FileOutput),
}

struct FileOutput {
    /// `None` once committed (or dropped); guards against double cleanup.
    writer: Option<BufWriter<File>>,
    temp: PathBuf,
    final_path: PathBuf,
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Output::Stdout(w) => w.write(buf),
            Output::File(f) => f.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Output::Stdout(w) => w.flush(),
            Output::File(f) => f.flush(),
        }
    }
}

impl Write for FileOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer
            .as_mut()
            .expect("writer is present until commit")
            .write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer
            .as_mut()
            .expect("writer is present until commit")
            .flush()
    }
}

impl Output {
    /// Flushes and, for a file, atomically publishes it. Consumes the output so
    /// the temp cannot be written again after the rename.
    fn commit(mut self) -> io::Result<()> {
        match &mut self {
            Output::Stdout(w) => w.flush(),
            Output::File(f) => f.commit(),
        }
    }
}

impl FileOutput {
    fn commit(&mut self) -> io::Result<()> {
        let writer = match self.writer.take() {
            Some(writer) => writer,
            None => return Ok(()),
        };
        // Flush and recover the File so the handle closes before the rename
        // (Windows refuses to rename an open file).
        let file = match writer.into_inner() {
            Ok(file) => file,
            Err(err) => {
                let _ = fs::remove_file(&self.temp);
                return Err(err.into_error());
            }
        };
        drop(file);
        if let Err(err) = fs::rename(&self.temp, &self.final_path) {
            let _ = fs::remove_file(&self.temp);
            return Err(err);
        }
        Ok(())
    }
}

impl Drop for FileOutput {
    fn drop(&mut self) {
        // Not committed: close the handle and remove the orphan temp so a
        // failed seal/open never leaves plaintext or ciphertext lying around.
        if let Some(writer) = self.writer.take() {
            let _ = writer.into_inner();
            let _ = fs::remove_file(&self.temp);
        }
    }
}

fn create_output(path: &str, force: bool) -> io::Result<Output> {
    if path == "-" {
        return Ok(Output::Stdout(BufWriter::new(io::stdout())));
    }
    let final_path = Path::new(path);
    if !force && final_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{path} already exists (pass --force to overwrite)"),
        ));
    }
    let parent = final_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temp = unique_temp_path(parent, final_path);
    let file = create_temp_file(&temp)?;
    Ok(Output::File(FileOutput {
        writer: Some(BufWriter::new(file)),
        temp,
        final_path: final_path.to_path_buf(),
    }))
}

/// A hidden sibling of the target with a random suffix, so the rename stays on
/// the same filesystem (atomic) and concurrent runs cannot collide.
fn unique_temp_path(parent: &Path, final_path: &Path) -> PathBuf {
    let name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    let mut rnd = [0u8; 8];
    OsRng.fill_bytes(&mut rnd);
    let suffix: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
    parent.join(format!(".{name}.tira-tmp-{suffix}"))
}

/// Creates the temp exclusively (`create_new`) with owner-only permissions on
/// Unix. Windows has no std 0600 equivalent; the file inherits the parent
/// directory's ACL, which is documented as a platform limitation.
fn create_temp_file(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Runs a streaming command into an atomic output, publishing only on success.
fn run_to_output<F>(path: &str, force: bool, write_fn: F) -> Result<u64, VaultError>
where
    F: FnOnce(&mut Output) -> Result<u64, VaultError>,
{
    let mut output = create_output(path, force)?;
    let bytes = write_fn(&mut output)?;
    output.commit()?;
    Ok(bytes)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) if err.is_empty() => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("error: {err}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let chunk_size = match args.chunk_mib.checked_mul(1024 * 1024) {
        Some(size) if size <= MAX_CHUNK_SIZE => size,
        _ => {
            eprintln!(
                "error: --chunk-mib {} is too large (max {} MiB)",
                args.chunk_mib,
                MAX_CHUNK_SIZE / (1024 * 1024)
            );
            return ExitCode::from(2);
        }
    };

    let passphrase = match passphrase() {
        Ok(passphrase) => passphrase,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };

    let mut input = match open_input(&args.input) {
        Ok(input) => input,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", args.input);
            return ExitCode::FAILURE;
        }
    };

    let result = match args.command {
        Command::Seal => {
            let path = args.output.expect("checked in parse_args");
            run_to_output(&path, args.force, |out| {
                seal_stream(&mut input, out, &passphrase, chunk_size)
            })
        }
        Command::Open => {
            let path = args.output.expect("checked in parse_args");
            run_to_output(&path, args.force, |out| {
                open_stream(&mut input, out, &passphrase)
            })
        }
        Command::Verify => open_stream(&mut input, io::sink(), &passphrase),
    };

    match result {
        Ok(bytes) => {
            match args.command {
                Command::Seal => eprintln!("sealed {bytes} bytes"),
                Command::Open => eprintln!("opened {bytes} bytes"),
                Command::Verify => eprintln!("verified {bytes} bytes"),
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique temp directory removed on drop, so tests never collide or leak.
    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let mut rnd = [0u8; 8];
            OsRng.fill_bytes(&mut rnd);
            let suffix: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
            let dir = std::env::temp_dir().join(format!("tira-seal-test-{suffix}"));
            fs::create_dir_all(&dir).unwrap();
            TestDir(dir)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
        fn temp_leftovers(&self) -> Vec<String> {
            fs::read_dir(&self.0)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.contains("tira-tmp"))
                .collect()
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn atomic_file_publishes_content_and_leaves_no_temp() {
        let dir = TestDir::new();
        let target = dir.path("out.bin");
        let bytes = run_to_output(target.to_str().unwrap(), false, |out| {
            out.write_all(b"hello atomic")?;
            Ok(12u64)
        })
        .unwrap();
        assert_eq!(bytes, 12);
        assert_eq!(fs::read(&target).unwrap(), b"hello atomic");
        assert!(dir.temp_leftovers().is_empty(), "temp sibling leaked");
    }

    #[test]
    fn failed_write_leaves_no_final_and_no_temp() {
        let dir = TestDir::new();
        let target = dir.path("out.bin");
        let result = run_to_output(target.to_str().unwrap(), false, |out| {
            out.write_all(b"partial")?;
            Err(VaultError::Decrypt)
        });
        assert!(result.is_err());
        assert!(!target.exists(), "final path must not appear on failure");
        assert!(dir.temp_leftovers().is_empty(), "temp leaked on failure");
    }

    #[test]
    fn force_does_not_destroy_existing_target_on_failure() {
        let dir = TestDir::new();
        let target = dir.path("out.bin");
        fs::write(&target, b"ORIGINAL").unwrap();
        // force=true but the stream fails: the original must survive untouched
        // (the old File::create truncated it before authentication ran).
        let result = run_to_output(target.to_str().unwrap(), true, |out| {
            out.write_all(b"replacement partial")?;
            Err(VaultError::Decrypt)
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(&target).unwrap(),
            b"ORIGINAL",
            "force must not truncate the target before success"
        );
        assert!(dir.temp_leftovers().is_empty());
    }

    #[test]
    fn force_replaces_existing_target_on_success() {
        let dir = TestDir::new();
        let target = dir.path("out.bin");
        fs::write(&target, b"OLD").unwrap();
        run_to_output(target.to_str().unwrap(), true, |out| {
            out.write_all(b"NEW")?;
            Ok(3u64)
        })
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"NEW");
        assert!(dir.temp_leftovers().is_empty());
    }

    #[test]
    fn no_force_refuses_existing_target() {
        let dir = TestDir::new();
        let target = dir.path("out.bin");
        fs::write(&target, b"EXISTING").unwrap();
        let result = run_to_output(target.to_str().unwrap(), false, |out| {
            out.write_all(b"nope")?;
            Ok(4u64)
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"EXISTING");
    }

    #[cfg(unix)]
    #[test]
    fn published_file_is_owner_only_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new();
        let target = dir.path("out.bin");
        run_to_output(target.to_str().unwrap(), false, |out| {
            out.write_all(b"secret plaintext")?;
            Ok(16u64)
        })
        .unwrap();
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "expected owner-only, got {:o}",
            mode & 0o777
        );
    }
}
