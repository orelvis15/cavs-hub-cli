//! Thin wrappers around the `git` executable: the config we write plus the
//! read-only plumbing the git-index uploader needs (refs, commit deltas,
//! per-commit changes, tree snapshots, blob reads).

use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Absolute path of the current repository's working tree, erroring clearly
/// when the command is not run inside a Git repository.
pub fn top_level() -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("running `git` (is Git installed and on PATH?)")?;
    if !out.status.success() {
        bail!("not inside a Git repository — run this from within your repo");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run `git <args…>` with inherited stdio so its output streams straight to
/// the user (used by the push/pull/clone/init wrappers). Errors when git
/// exits non-zero, surfacing the same exit intent to the caller.
pub fn run_inherit(args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running `git` (is Git installed and on PATH?)")?;
    if !status.success() {
        bail!("`git {}` failed", args.join(" "));
    }
    Ok(())
}

/// Set a repository-local Git config key.
pub fn set_config(key: &str, value: &str) -> Result<()> {
    let out = Command::new("git")
        .args(["config", key, value])
        .output()
        .with_context(|| format!("running `git config {key}`"))?;
    if !out.status.success() {
        bail!(
            "`git config {key}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Run `git lfs install --local` so this repository has the LFS filters and
/// hooks (clean/smudge/process) wired up. Without them git-lfs skips object
/// checkout on clone/pull ("Git LFS is not installed for this repository").
/// Idempotent; safe to run on every connect.
pub fn lfs_install_local() -> Result<()> {
    let out = Command::new("git")
        .args(["lfs", "install", "--local"])
        .output()
        .context("running `git lfs install` (is git-lfs installed?)")?;
    if !out.status.success() {
        bail!(
            "`git lfs install --local` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Read a repository-local Git config key (None when unset).
pub fn get_config(key: &str) -> Result<Option<String>> {
    let out = Command::new("git")
        .args(["config", "--get", key])
        .output()
        .with_context(|| format!("running `git config --get {key}`"))?;
    if !out.status.success() {
        return Ok(None);
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if v.is_empty() { None } else { Some(v) })
}

/// Absolute path of the repository's .git directory.
pub fn git_dir() -> Result<PathBuf> {
    let out = run_git(&["rev-parse", "--absolute-git-dir"])?;
    Ok(PathBuf::from(out.trim()))
}

fn run_git(args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("running `git {}`", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_git_bytes(args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd
        .spawn()
        .with_context(|| format!("running `git {}`", args.join(" ")))?;
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("no stdin pipe"))?
            .write_all(input)
            .context("writing to git stdin")?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// One local ref (branch or tag) with its peeled commit sha.
#[derive(Debug, Clone)]
pub struct RefInfo {
    /// Short name (`main`, `v1.2.0`).
    pub name: String,
    /// `branch` or `tag`.
    pub kind: String,
    /// The commit the ref points at (peeled for annotated tags).
    pub commit: String,
    /// Tag message subject (annotated tags only).
    pub message: String,
    /// Tagger date, ISO-8601 (annotated tags only; may be empty).
    pub tagged_at: String,
}

/// List local branches and tags with peeled commit shas.
pub fn for_each_ref() -> Result<Vec<RefInfo>> {
    let out = run_git(&[
        "for-each-ref",
        "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(contents:subject)%00%(taggerdate:iso8601-strict)",
        "refs/heads",
        "refs/tags",
    ])?;
    let mut refs = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split('\0').collect();
        if parts.len() < 3 {
            continue;
        }
        let (full, obj, peeled) = (parts[0], parts[1], parts[2]);
        let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
            ("branch", n)
        } else if let Some(n) = full.strip_prefix("refs/tags/") {
            ("tag", n)
        } else {
            continue;
        };
        let commit = if peeled.is_empty() { obj } else { peeled };
        refs.push(RefInfo {
            name: name.to_string(),
            kind: kind.to_string(),
            commit: commit.to_string(),
            message: parts.get(3).unwrap_or(&"").to_string(),
            tagged_at: parts.get(4).unwrap_or(&"").to_string(),
        });
    }
    Ok(refs)
}

/// Does the object exist locally as a commit?
pub fn commit_exists(sha: &str) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", &format!("{sha}^{{commit}}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Commits reachable from `tips` but not from `known`, oldest first.
pub fn rev_list_delta(tips: &[String], known: &[String]) -> Result<Vec<String>> {
    if tips.is_empty() {
        return Ok(Vec::new());
    }
    let mut args: Vec<String> = vec!["rev-list".into(), "--topo-order".into(), "--reverse".into()];
    args.extend(tips.iter().cloned());
    if !known.is_empty() {
        args.push("--not".into());
        args.extend(known.iter().cloned());
    }
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = run_git(&arg_refs)?;
    Ok(out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Commit metadata as read by the index uploader.
#[derive(Debug, Clone)]
pub struct CommitMeta {
    pub sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    pub committed_at: String,
    pub message: String,
}

/// Batch-read commit metadata for a set of shas (one git process).
pub fn commit_metadata(shas: &[String]) -> Result<Vec<CommitMeta>> {
    if shas.is_empty() {
        return Ok(Vec::new());
    }
    let stdin = shas.join("\n");
    let out = run_git_bytes(
        &[
            "log",
            "--no-walk=unsorted",
            "--stdin",
            // Records separated by \x1e, fields by \x01. %B is the raw body.
            "--format=%x1e%H%x01%P%x01%an%x01%ae%x01%aI%x01%cI%x01%B",
        ],
        Some(stdin.as_bytes()),
    )?;
    let text = String::from_utf8_lossy(&out);
    let mut commits = Vec::new();
    for record in text.split('\x1e') {
        let record = record.trim_start_matches('\n');
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.splitn(7, '\x01').collect();
        if fields.len() < 7 {
            continue;
        }
        commits.push(CommitMeta {
            sha: fields[0].to_string(),
            parents: fields[1].split_whitespace().map(str::to_string).collect(),
            author_name: fields[2].to_string(),
            author_email: fields[3].to_string(),
            authored_at: fields[4].to_string(),
            committed_at: fields[5].to_string(),
            message: fields[6].trim_end().to_string(),
        });
    }
    Ok(commits)
}

/// One changed path in a commit's (first-parent) diff.
#[derive(Debug, Clone)]
pub struct DiffEntry {
    pub path: String,
    pub status: char, // A|M|D (others normalized by the caller)
    pub old_blob: String,
    pub new_blob: String,
}

const ZERO_OID_40: &str = "0000000000000000000000000000000000000000";

/// Batch first-parent diffs for many commits in ONE git process:
/// `git diff-tree --stdin -r -z --root -m --first-parent` emits, per commit,
/// its sha followed by raw entries.
pub fn diff_trees(shas: &[String]) -> Result<HashMap<String, Vec<DiffEntry>>> {
    let mut out_map: HashMap<String, Vec<DiffEntry>> = HashMap::new();
    if shas.is_empty() {
        return Ok(out_map);
    }
    let stdin = shas.join("\n");
    let raw = run_git_bytes(
        &[
            "diff-tree",
            "--stdin",
            "-r",
            "-z",
            "--root",
            "-m",
            "--first-parent",
            "--no-renames",
        ],
        Some(stdin.as_bytes()),
    )?;
    let text = String::from_utf8_lossy(&raw);
    let mut current = String::new();
    let mut tokens = text.split('\0').peekable();
    while let Some(tok) = tokens.next() {
        let tok = tok.trim_start_matches('\n');
        if tok.is_empty() {
            continue;
        }
        if let Some(rest) = tok.strip_prefix(':') {
            // ":oldmode newmode oldsha newsha status" then the path token.
            let fields: Vec<&str> = rest.split_whitespace().collect();
            let path = tokens.next().unwrap_or("").to_string();
            if fields.len() < 5 || path.is_empty() {
                continue;
            }
            let status = fields[4].chars().next().unwrap_or('M');
            let entry = DiffEntry {
                path,
                status,
                old_blob: fields[2].to_string(),
                new_blob: fields[3].to_string(),
            };
            out_map.entry(current.clone()).or_default().push(entry);
        } else {
            // A commit header: the sha diff-tree echoes before its entries.
            let sha = tok.split_whitespace().next().unwrap_or("");
            if sha.len() >= 40 && sha != ZERO_OID_40 {
                current = sha.to_string();
                out_map.entry(current.clone()).or_default();
            }
        }
    }
    Ok(out_map)
}

/// One blob in a tree snapshot.
#[derive(Debug, Clone)]
pub struct TreeBlob {
    pub path: String,
    pub blob: String,
    pub size: u64,
}

/// Recursively list a tree's blobs with sizes (`git ls-tree -r -l -z`).
pub fn ls_tree(commit: &str) -> Result<Vec<TreeBlob>> {
    let raw = run_git_bytes(&["ls-tree", "-r", "-l", "-z", commit], None)?;
    let text = String::from_utf8_lossy(&raw);
    let mut blobs = Vec::new();
    for record in text.split('\0') {
        if record.is_empty() {
            continue;
        }
        // "<mode> <type> <sha> <size>\t<path>"
        let Some((meta, path)) = record.split_once('\t') else {
            continue;
        };
        let fields: Vec<&str> = meta.split_whitespace().collect();
        if fields.len() < 4 || fields[1] != "blob" {
            continue;
        }
        let size = fields[3].parse::<u64>().unwrap_or(u64::MAX);
        blobs.push(TreeBlob {
            path: path.to_string(),
            blob: fields[2].to_string(),
            size,
        });
    }
    Ok(blobs)
}

/// Batch-read blob contents (`git cat-file --batch`), returning a map of
/// blob-sha → bytes. Missing/non-blob objects are skipped.
pub fn read_blobs(shas: &[String]) -> Result<HashMap<String, Vec<u8>>> {
    let mut out_map = HashMap::new();
    if shas.is_empty() {
        return Ok(out_map);
    }
    let stdin = shas.join("\n");
    let raw = run_git_bytes(&["cat-file", "--batch"], Some(stdin.as_bytes()))?;
    let mut i = 0usize;
    while i < raw.len() {
        // Header line: "<sha> <type> <size>\n" or "<sha> missing\n".
        let nl = match raw[i..].iter().position(|&b| b == b'\n') {
            Some(n) => i + n,
            None => break,
        };
        let header = String::from_utf8_lossy(&raw[i..nl]).into_owned();
        i = nl + 1;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() < 3 || fields[1] != "blob" {
            continue; // "missing" and non-blob headers carry no payload
        }
        let size: usize = fields[2].parse().unwrap_or(0);
        if i + size > raw.len() {
            break;
        }
        out_map.insert(fields[0].to_string(), raw[i..i + size].to_vec());
        i += size + 1; // trailing LF after the payload
    }
    Ok(out_map)
}

/// Filter shas that are blobs no larger than `max` (`cat-file --batch-check`).
pub fn small_blobs(shas: &[String], max: u64) -> Result<Vec<String>> {
    let mut out_vec = Vec::new();
    if shas.is_empty() {
        return Ok(out_vec);
    }
    let stdin = shas.join("\n");
    let raw = run_git_bytes(&["cat-file", "--batch-check"], Some(stdin.as_bytes()))?;
    for line in String::from_utf8_lossy(&raw).lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 3 && fields[1] == "blob" {
            if let Ok(size) = fields[2].parse::<u64>() {
                if size <= max {
                    out_vec.push(fields[0].to_string());
                }
            }
        }
    }
    Ok(out_vec)
}

/// Locate an executable on `PATH`, honouring the `.exe` suffix on Windows.
pub fn find_on_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{bin}.exe"));
            if is_executable(&exe) {
                return Some(exe);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    p.is_file()
}
