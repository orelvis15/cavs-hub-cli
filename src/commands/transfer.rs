//! Direct object transfer without git: `upload`, `download`, `verify`.
//!
//! These use the content-addressed data plane directly (upload sessions +
//! presigned URLs), so scripts and CI can push/pull individual artifacts
//! without a working tree. Objects are identified by their SHA-256 (the CAVS
//! oid), computed locally here.
//!
//! Large files are never buffered whole in memory: hashing streams the file in
//! fixed-size chunks, uploads stream the body straight from a file handle, and
//! downloads stream the response into a temporary file (hashing as they go)
//! before an atomic rename into place.

use crate::api::Client;
use crate::config::Config;
use crate::error::{err, Category};
use crate::info;
use crate::output;
use crate::util;
use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Streaming hash buffer size. 64 KiB is a good balance between syscall count
/// and memory footprint, and keeps us far away from loading whole files.
const CHUNK: usize = 64 * 1024;

// --- cav upload -------------------------------------------------------------

#[derive(clap::Args)]
pub struct UploadArgs {
    /// Files to upload to the connected repository.
    #[arg(required = true)]
    files: Vec<String>,
}

#[derive(Serialize)]
struct UploadedObject {
    oid: String,
    path: String,
    size: i64,
}

pub fn upload(cfg: Config, args: UploadArgs) -> Result<()> {
    util::require_login(&cfg)?;
    let repo_id = util::connected_repo_id()?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());

    // Hash every file first (streaming, no whole-file buffering) so a bad path
    // fails before we open a session. We keep only oid + size + path per file,
    // never the bytes.
    struct Local {
        path: String,
        oid: String,
        size: i64,
    }
    let mut locals = Vec::with_capacity(args.files.len());
    for path in &args.files {
        let oid = sha256_file(path)?;
        let size = File::open(path)
            .and_then(|f| f.metadata())
            .map_err(|e| err(Category::InvalidPath, format!("reading {path}: {e}")))?
            .len() as i64;
        locals.push(Local {
            path: path.clone(),
            oid,
            size,
        });
    }

    let session = client
        .create_upload_session(&repo_id, locals.len())
        .context("opening upload session")?;

    let pairs: Vec<(String, i64)> = locals.iter().map(|l| (l.oid.clone(), l.size)).collect();
    let authorized = client
        .authorize_objects(&repo_id, &session, &pairs)
        .context("authorizing objects")?;

    for a in &authorized {
        if let Some(e) = &a.error {
            return Err(err(
                Category::ApiError,
                format!("object {} rejected: {e}", short(&a.oid)),
            ));
        }
    }

    let mut uploaded = Vec::with_capacity(locals.len());
    for l in &locals {
        let auth = authorized
            .iter()
            .find(|a| a.oid == l.oid)
            .with_context(|| format!("no upload URL returned for {}", l.path))?;
        // Stream the body straight from a freshly-opened file handle.
        let file = File::open(&l.path)
            .map_err(|e| err(Category::InvalidPath, format!("opening {}: {e}", l.path)))?;
        client
            .put_presigned_reader(&auth.upload_url, file, l.size as u64)
            .with_context(|| format!("uploading {}", l.path))?;
        client
            .complete_object(&repo_id, &session, &l.oid, l.size)
            .with_context(|| format!("completing {}", l.path))?;
        info!(
            "uploaded {}  {}  ({})",
            short(&l.oid),
            l.path,
            util::bytes(l.size)
        );
        uploaded.push(UploadedObject {
            oid: l.oid.clone(),
            path: l.path.clone(),
            size: l.size,
        });
    }

    client
        .finalize_upload(&repo_id, &session)
        .context("finalizing upload session")?;

    if output::is_json() {
        output::emit_json(&uploaded)?;
    } else if !output::is_quiet() {
        println!("\n{} object(s) uploaded.", uploaded.len());
    }
    Ok(())
}

// --- cav download -----------------------------------------------------------

#[derive(clap::Args)]
pub struct DownloadArgs {
    /// The object SHA-256 (oid) to download.
    oid: String,
    /// Write to this path (defaults to the oid in the current directory).
    #[arg(long, short)]
    output: Option<String>,
    /// Overwrite the output file if it already exists.
    #[arg(long, short)]
    force: bool,
}

#[derive(Serialize)]
struct DownloadedObject {
    oid: String,
    path: String,
    size: i64,
}

pub fn download(cfg: Config, args: DownloadArgs) -> Result<()> {
    util::require_login(&cfg)?;
    if !is_sha256(&args.oid) {
        return Err(err(
            Category::InvalidPath,
            format!("expected a 64-character SHA-256 oid, got {:?}", args.oid),
        ));
    }
    let oid = args.oid.to_ascii_lowercase();
    let out = args.output.clone().unwrap_or_else(|| oid.clone());

    // Overwrite guard: refuse to clobber an existing file without --force.
    if Path::new(&out).exists() && !args.force {
        return Err(err(
            Category::InvalidPath,
            format!("{out} already exists — pass --force to overwrite"),
        ));
    }

    let repo_id = util::connected_repo_id()?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());

    let authorized = client
        .authorize_download(&repo_id, std::slice::from_ref(&args.oid))
        .context("authorizing download")?;
    let obj = authorized
        .into_iter()
        .next()
        .context("empty download authorization response")?;
    if let Some(e) = &obj.error {
        return Err(err(
            Category::MissingObject,
            format!("object {} not available: {e}", short(&oid)),
        ));
    }

    // Stream into a sibling temp file, hashing as we go. The temp name is
    // derived from the oid (no rand/timestamps) so it lands in the same
    // directory and a stale one is recognisable.
    let tmp = temp_path(&out, &oid);
    let mut file = File::create(&tmp).map_err(|e| {
        err(
            Category::InvalidPath,
            format!("creating {}: {e}", tmp.display()),
        )
    })?;
    let mut hasher = HashingWriter::new(&mut file);
    let copied = client
        .get_presigned_to_writer(&obj.download_url, &mut hasher)
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
    let got = hasher.hex();
    file.flush().ok();
    drop(file);

    // Integrity check: the content must hash to the requested oid.
    if got != oid {
        let _ = std::fs::remove_file(&tmp);
        return Err(err(
            Category::ChecksumMismatch,
            format!(
                "integrity check failed: downloaded content hashes to {}",
                short(&got)
            ),
        ));
    }

    // Atomic rename temp → final path.
    std::fs::rename(&tmp, &out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        err(Category::InvalidPath, format!("finalizing {out}: {e}"))
    })?;

    if output::is_json() {
        output::emit_json(&DownloadedObject {
            oid: oid.clone(),
            path: out.clone(),
            size: copied as i64,
        })?;
    } else if !output::is_quiet() {
        println!(
            "downloaded {} → {} ({})",
            short(&oid),
            out,
            util::bytes(copied as i64)
        );
    }
    Ok(())
}

// --- cav verify -------------------------------------------------------------

#[derive(clap::Args)]
pub struct VerifyArgs {
    /// Local files to verify against the Hub.
    #[arg(required = true)]
    files: Vec<String>,
}

#[derive(Serialize)]
struct VerifyEntry {
    path: String,
    oid: String,
    present: bool,
}

pub fn verify(cfg: Config, args: VerifyArgs) -> Result<()> {
    util::require_login(&cfg)?;

    // Hash locally (streaming) first, so a bad path fails fast — before any
    // network or the repo-connection check.
    let mut oids = Vec::with_capacity(args.files.len());
    for path in &args.files {
        let oid = sha256_file(path)?;
        oids.push((path.clone(), oid));
    }

    let repo_id = util::connected_repo_id()?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let authorized = client
        .authorize_download(
            &repo_id,
            &oids.iter().map(|(_, o)| o.clone()).collect::<Vec<_>>(),
        )
        .context("checking objects against the Hub")?;

    let mut entries = Vec::with_capacity(oids.len());
    let mut missing = 0;
    for (path, oid) in &oids {
        let present = authorized
            .iter()
            .any(|a| &a.oid == oid && a.error.is_none());
        if !present {
            missing += 1;
        }
        entries.push(VerifyEntry {
            path: path.clone(),
            oid: oid.clone(),
            present,
        });
    }

    if output::is_json() {
        // Plan §7 requires the JSON array of {path, oid, present}.
        output::emit_json(&entries)?;
    } else {
        for e in &entries {
            if e.present {
                info!("  \u{2713} {}  {}", e.path, short(&e.oid));
            } else {
                info!("  \u{2717} {}  {} (not on the Hub)", e.path, short(&e.oid));
            }
        }
    }

    if missing > 0 {
        return Err(err(
            Category::MissingObject,
            format!("{missing} file(s) are not present on the Hub"),
        ));
    }
    if !output::is_json() && !output::is_quiet() {
        println!("\nAll {} file(s) verified.", entries.len());
    }
    Ok(())
}

// --- helpers ----------------------------------------------------------------

/// A `Write` adapter that streams everything written to it through a running
/// SHA-256 while forwarding it to the inner writer, and tallies the byte count.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
    len: u64,
}

impl<W: Write> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            len: 0,
        }
    }

    fn hex(self) -> String {
        hex::encode(self.hasher.finalize())
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.len += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Hash a reader in fixed-size chunks, never holding the whole content.
fn sha256_reader<R: Read>(mut r: R) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = [0u8; CHUNK];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Stream-hash a file on disk to its CAVS oid, mapping I/O errors to a clear
/// `InvalidPath` category.
fn sha256_file(path: &str) -> Result<String> {
    let file =
        File::open(path).map_err(|e| err(Category::InvalidPath, format!("reading {path}: {e}")))?;
    sha256_reader(file).map_err(|e| err(Category::InvalidPath, format!("hashing {path}: {e}")))
}

/// Derive the temporary download path from the output path and oid. Non-random
/// and in the same directory as the target so the final rename is atomic.
fn temp_path(out: &str, oid: &str) -> PathBuf {
    let tag: String = oid.chars().take(16).collect();
    let mut p = PathBuf::from(out);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());
    p.set_file_name(format!("{name}.cavs-partial-{tag}"));
    p
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn short(oid: &str) -> String {
    oid.chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn sha256_matches_known_vectors() {
        // Empty input.
        assert_eq!(
            sha256_reader(Cursor::new(b"")).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // "abc".
        assert_eq!(
            sha256_reader(Cursor::new(b"abc")).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_streaming_survives_chunk_boundaries() {
        // Larger than one CHUNK so the read loop runs multiple iterations.
        let data = vec![0x61u8; CHUNK * 2 + 123];
        let mut direct = Sha256::new();
        direct.update(&data);
        let expected = hex::encode(direct.finalize());
        assert_eq!(sha256_reader(Cursor::new(&data)).unwrap(), expected);
    }

    #[test]
    fn hashing_writer_hashes_and_counts() {
        let mut sink = Vec::new();
        let mut hw = HashingWriter::new(&mut sink);
        hw.write_all(b"abc").unwrap();
        assert_eq!(hw.len, 3);
        assert_eq!(
            hw.hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sink, b"abc");
    }

    #[test]
    fn is_sha256_validates_length_and_alphabet() {
        assert!(is_sha256(&"a".repeat(64)));
        assert!(is_sha256(&"0123456789abcdef".repeat(4)));
        assert!(!is_sha256(&"a".repeat(63)));
        assert!(!is_sha256(&"a".repeat(65)));
        assert!(!is_sha256(&"g".repeat(64))); // non-hex
        assert!(!is_sha256(""));
    }

    #[test]
    fn temp_path_is_deterministic_and_sibling() {
        let oid = "a".repeat(64);
        let p = temp_path("out/model.bin", &oid);
        assert_eq!(
            p,
            PathBuf::from("out/model.bin.cavs-partial-aaaaaaaaaaaaaaaa")
        );
        // Same directory as the target (atomic rename requires it).
        assert_eq!(p.parent(), Path::new("out/model.bin").parent());
        // Deterministic.
        assert_eq!(p, temp_path("out/model.bin", &oid));
    }
}
