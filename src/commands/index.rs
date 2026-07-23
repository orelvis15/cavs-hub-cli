//! `cav repo index` and the hidden `cav hook pre-push` entrypoint.
//!
//! CAVS Node's data plane is content-addressed — it never sees paths, refs or
//! commits. This module is the bridge: it walks the local git repository,
//! extracts LFS pointers (path → sha256 oid) and commit metadata, and uploads
//! the delta to the Hub's git-index API so the dashboard can render a
//! GitHub-style Files / Commits / Branches view.
//!
//! Invocations:
//!   cav repo index [--full] [--ref NAME]...   manual / initial backfill
//!   cav hook pre-push <remote> <url>          best-effort, from the pre-push
//!                                             hook installed by `repo connect`
//!
//! The Hub is the source of incremental state (`GET /index/state`): we upload
//! only commits it does not know yet, and a full tree snapshot per pushed ref
//! (snapshots are replace-on-push server-side, so they are always correct even
//! after force-pushes).

use crate::api::{
    Client, IndexArtifact, IndexCommit, IndexFinalize, IndexRefHead, IndexStats, IndexTag,
    IndexTreeEntry,
};
use crate::config::Config;
use crate::git;
use crate::lfs;
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::io::Read;

const COMMITS_PER_PAGE: usize = 500;
const TREE_ENTRIES_PER_PAGE: usize = 5000;
/// Commit cap per index run — a backstop against pathological histories.
const MAX_COMMITS_PER_RUN: usize = 100_000;

#[derive(clap::Args)]
pub struct IndexArgs {
    /// Index the full history of every branch and tag (initial backfill).
    #[arg(long)]
    full: bool,

    /// Only index these refs (default: all branches and tags).
    #[arg(long = "ref", value_name = "NAME")]
    refs: Vec<String>,

    /// Print what would be uploaded without contacting the Hub.
    #[arg(long)]
    dry_run: bool,
}

impl IndexArgs {
    /// The `--full` invocation `cav repo connect` runs as the initial backfill.
    pub fn full_backfill() -> Self {
        Self {
            full: true,
            refs: Vec::new(),
            dry_run: false,
        }
    }
}

/// A ref update to index: name/kind/tip, or a deletion.
struct RefTarget {
    name: String,
    kind: String, // branch|tag
    tip: String,  // commit sha ("" for deletions)
    deleted: bool,
    tag_message: String,
    tagged_at: String,
}

pub fn run_index(cfg: Config, args: IndexArgs) -> Result<()> {
    let (client, repo_id) = hub_client(&cfg)?;

    let local = git::for_each_ref().context("listing local refs")?;
    let selected: Vec<&git::RefInfo> = if args.refs.is_empty() {
        local.iter().collect()
    } else {
        let want: HashSet<&str> = args.refs.iter().map(|s| s.as_str()).collect();
        local
            .iter()
            .filter(|r| want.contains(r.name.as_str()))
            .collect()
    };
    if selected.is_empty() {
        bail!("no matching local refs to index");
    }

    let targets: Vec<RefTarget> = selected
        .iter()
        .map(|r| RefTarget {
            name: r.name.clone(),
            kind: r.kind.clone(),
            tip: r.commit.clone(),
            deleted: false,
            tag_message: r.message.clone(),
            tagged_at: r.tagged_at.clone(),
        })
        .collect();

    upload(&client, &repo_id, targets, args.full, args.dry_run, true)
}

/// Entry point for the pre-push hook. Reads the ref lines git feeds the hook
/// on stdin (`<local-ref> <local-sha> <remote-ref> <remote-sha>`) and indexes
/// exactly the refs being pushed. Never fails the push: the caller wraps us in
/// `|| true`, and we additionally swallow errors into a one-line warning.
pub fn run_hook_pre_push(cfg: Config, _remote: String, _url: String) -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok();

    if let Err(err) = hook_inner(cfg, &input) {
        eprintln!("cav: git index upload skipped: {err:#}");
    }
    Ok(())
}

fn hook_inner(cfg: Config, stdin: &str) -> Result<()> {
    let mut targets = Vec::new();
    let local_refs = git::for_each_ref().unwrap_or_default();
    let by_name: HashMap<&str, &git::RefInfo> =
        local_refs.iter().map(|r| (r.name.as_str(), r)).collect();

    for line in stdin.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 4 {
            continue;
        }
        let (_local_ref, local_sha, remote_ref, _remote_sha) =
            (parts[0], parts[1], parts[2], parts[3]);
        let (kind, name) = if let Some(n) = remote_ref.strip_prefix("refs/heads/") {
            ("branch", n)
        } else if let Some(n) = remote_ref.strip_prefix("refs/tags/") {
            ("tag", n)
        } else {
            continue;
        };
        let deleted = local_sha.bytes().all(|b| b == b'0');
        let info = by_name.get(name);
        // Annotated tags: local_sha is the tag object; use the peeled commit.
        let tip = info
            .map(|r| r.commit.clone())
            .unwrap_or_else(|| local_sha.to_string());
        targets.push(RefTarget {
            name: name.to_string(),
            kind: kind.to_string(),
            tip: if deleted { String::new() } else { tip },
            deleted,
            tag_message: info.map(|r| r.message.clone()).unwrap_or_default(),
            tagged_at: info.map(|r| r.tagged_at.clone()).unwrap_or_default(),
        });
    }
    if targets.is_empty() {
        return Ok(());
    }

    let (client, repo_id) = hub_client(&cfg)?;
    upload(&client, &repo_id, targets, false, false, false)
}

/// Resolve the Hub client + repository id from the repo-local git config
/// written by `cav repo connect`.
fn hub_client(cfg: &Config) -> Result<(Client, String)> {
    let repo_id = git::get_config("cavs.repo-id")?
        .context("repository is not connected — run `cav repo connect <org>/<repo>` first")?;
    let api_base = git::get_config("cavs.api-base")?.unwrap_or_else(|| cfg.api_base.clone());
    let token = match std::env::var("CAVS_TOKEN") {
        Ok(t) if !t.is_empty() => Some(t),
        _ => cfg.token.clone(),
    };
    if token.is_none() {
        bail!("not logged in — run `cav login` (or set $CAVS_TOKEN)");
    }
    Ok((Client::new(api_base, token), repo_id))
}

fn upload(
    client: &Client,
    repo_id: &str,
    targets: Vec<RefTarget>,
    full: bool,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    // Ask the Hub what it already knows so we only ship the delta.
    let state = client
        .index_state(repo_id)
        .context("fetching index state from the Hub")?;
    let hub_heads: HashMap<String, String> = state
        .refs
        .into_iter()
        .map(|r| (r.name, r.head_sha))
        .collect();

    // Skip refs whose tip the Hub already has (unless --full re-index).
    let mut work: Vec<&RefTarget> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();
    for t in &targets {
        if t.deleted {
            deleted.push(t.name.clone());
            continue;
        }
        if !full && hub_heads.get(&t.name).map(String::as_str) == Some(t.tip.as_str()) {
            continue;
        }
        work.push(t);
    }
    if work.is_empty() && deleted.is_empty() {
        if verbose {
            println!("Git index is already up to date.");
        }
        return Ok(());
    }

    // Commit delta: reachable from the pushed tips, minus what the Hub knows
    // (heads that still exist locally). --full ignores hub heads and walks all.
    let tips: Vec<String> = work.iter().map(|t| t.tip.clone()).collect();
    let known: Vec<String> = if full {
        Vec::new()
    } else {
        hub_heads
            .values()
            .filter(|sha| !sha.is_empty() && git::commit_exists(sha))
            .cloned()
            .collect()
    };
    let mut shas = git::rev_list_delta(&tips, &known).context("computing commit delta")?;
    if shas.len() > MAX_COMMITS_PER_RUN {
        eprintln!(
            "cav: history has {} new commits; indexing the most recent {}",
            shas.len(),
            MAX_COMMITS_PER_RUN
        );
        shas = shas.split_off(shas.len() - MAX_COMMITS_PER_RUN);
    }

    let commits = build_commits(&shas)?;
    let trees = build_trees(&work)?;
    let total_files: usize = trees.values().map(Vec::len).sum();

    if dry_run {
        println!(
            "Would upload {} commit(s), {} ref(s) with {} LFS file(s), {} deleted ref(s).",
            commits.len(),
            work.len(),
            total_files,
            deleted.len()
        );
        return Ok(());
    }

    // Head-commit dates for finalize (one batch lookup for all tips).
    let tip_meta = git::commit_metadata(&tips).unwrap_or_default();
    let tip_dates: HashMap<&str, &str> = tip_meta
        .iter()
        .map(|c| (c.sha.as_str(), c.committed_at.as_str()))
        .collect();

    let session = client
        .create_index_session(repo_id)
        .context("creating index session")?;

    for page in commits.chunks(COMMITS_PER_PAGE) {
        client
            .push_commits(repo_id, &session.session_id, page)
            .context("uploading commits")?;
    }
    for t in &work {
        if let Some(entries) = trees.get(&t.name) {
            if entries.is_empty() {
                // Still upload an empty page so the server flips the ref to an
                // empty snapshot (e.g. all LFS files were removed).
                client
                    .push_tree_page(repo_id, &session.session_id, &t.name, &[])
                    .ok();
            }
            for page in entries.chunks(TREE_ENTRIES_PER_PAGE) {
                client
                    .push_tree_page(repo_id, &session.session_id, &t.name, page)
                    .with_context(|| format!("uploading tree of {}", t.name))?;
            }
        }
    }

    let now_fallback = || {
        tip_meta
            .first()
            .map(|c| c.committed_at.clone())
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
    };
    let finalize = IndexFinalize {
        refs: work
            .iter()
            .map(|t| IndexRefHead {
                name: t.name.clone(),
                kind: t.kind.clone(),
                head_sha: t.tip.clone(),
                head_committed_at: tip_dates
                    .get(t.tip.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(now_fallback),
            })
            .collect(),
        deleted_refs: deleted.clone(),
        tags: work
            .iter()
            .filter(|t| t.kind == "tag")
            .map(|t| IndexTag {
                tag: t.name.clone(),
                target_sha: t.tip.clone(),
                message: t.tag_message.clone(),
                tagged_at: if t.tagged_at.is_empty() {
                    tip_dates
                        .get(t.tip.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(now_fallback)
                } else {
                    t.tagged_at.clone()
                },
            })
            .collect(),
        stats: IndexStats {
            commits: commits.len(),
            files: total_files,
        },
    };
    client
        .finalize_index(repo_id, &session.session_id, &finalize)
        .context("finalizing index")?;

    if verbose {
        println!(
            "Indexed {} commit(s) and {} ref(s) ({} LFS file(s)).",
            commits.len(),
            work.len(),
            total_files
        );
    } else {
        eprintln!(
            "cav: indexed {} commit(s), {} ref(s)",
            commits.len(),
            work.len()
        );
    }
    Ok(())
}

/// Build the commit payloads: metadata plus per-commit changed LFS artifacts.
fn build_commits(shas: &[String]) -> Result<Vec<IndexCommit>> {
    if shas.is_empty() {
        return Ok(Vec::new());
    }
    let meta = git::commit_metadata(shas).context("reading commit metadata")?;
    let diffs = git::diff_trees(shas).context("reading commit diffs")?;

    // Every blob sha that might be an LFS pointer, across all commits.
    let mut candidates: HashSet<String> = HashSet::new();
    for entries in diffs.values() {
        for e in entries {
            for sha in [&e.old_blob, &e.new_blob] {
                if !sha.is_empty() && !sha.bytes().all(|b| b == b'0') {
                    candidates.insert(sha.clone());
                }
            }
        }
    }
    let candidate_list: Vec<String> = candidates.into_iter().collect();
    let small = git::small_blobs(&candidate_list, lfs::MAX_POINTER_SIZE)?;
    let blobs = git::read_blobs(&small)?;
    let pointers: HashMap<&str, lfs::Pointer> = blobs
        .iter()
        .filter_map(|(sha, bytes)| lfs::parse_pointer(bytes).map(|p| (sha.as_str(), p)))
        .collect();

    let mut out = Vec::with_capacity(meta.len());
    for c in meta {
        let mut artifacts = Vec::new();
        let mut files_changed = 0usize;
        if let Some(entries) = diffs.get(&c.sha) {
            files_changed = entries.len();
            for e in entries {
                let new_ptr = pointers.get(e.new_blob.as_str());
                let old_ptr = pointers.get(e.old_blob.as_str());
                if new_ptr.is_none() && old_ptr.is_none() {
                    continue; // not an LFS-tracked file
                }
                let status = match e.status {
                    'A' => "A",
                    'D' => "D",
                    _ => "M",
                };
                artifacts.push(IndexArtifact {
                    path: e.path.clone(),
                    status: status.to_string(),
                    oid: new_ptr.map(|p| p.oid.clone()).unwrap_or_default(),
                    prev_oid: old_ptr.map(|p| p.oid.clone()).unwrap_or_default(),
                    size: new_ptr.map(|p| p.size as i64).unwrap_or(0),
                    prev_size: old_ptr.map(|p| p.size as i64).unwrap_or(0),
                });
            }
        }
        out.push(IndexCommit {
            sha: c.sha,
            parents: c.parents,
            author_name: c.author_name,
            author_email: c.author_email,
            authored_at: c.authored_at,
            committed_at: c.committed_at,
            message: c.message,
            artifacts,
            files_changed,
        });
    }
    Ok(out)
}

/// Build the LFS-pointer tree snapshot of every target ref tip.
fn build_trees(targets: &[&RefTarget]) -> Result<HashMap<String, Vec<IndexTreeEntry>>> {
    let mut out = HashMap::new();
    for t in targets {
        let blobs = git::ls_tree(&t.tip).with_context(|| format!("listing tree of {}", t.name))?;
        let small: Vec<git::TreeBlob> = blobs
            .into_iter()
            .filter(|b| b.size <= lfs::MAX_POINTER_SIZE)
            .collect();
        let shas: Vec<String> = small.iter().map(|b| b.blob.clone()).collect();
        let contents = git::read_blobs(&shas)?;
        let mut entries = Vec::new();
        for b in small {
            if let Some(bytes) = contents.get(&b.blob) {
                if let Some(p) = lfs::parse_pointer(bytes) {
                    entries.push(IndexTreeEntry {
                        path: b.path,
                        oid: p.oid,
                        size: p.size as i64,
                    });
                }
            }
        }
        out.insert(t.name.clone(), entries);
    }
    Ok(out)
}
