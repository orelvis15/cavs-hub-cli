//! Thin Git wrappers: `init`, `clone`, `push`, `pull`, `sync`.
//!
//! These delegate the heavy lifting to `git` (and the CAVS LFS transfer agent
//! wired by `cav repo connect`); the CLI's job is to make the connected-repo
//! workflow one command instead of several. Extra args after `--` are passed
//! straight through to git.

use crate::commands::repo::{self, ConnectArgs};
use crate::config::Config;
use crate::git;
use anyhow::{bail, Result};

#[derive(clap::Args)]
pub struct InitArgs {
    /// Directory to initialize (defaults to the current directory).
    path: Option<String>,
}

pub fn init(_cfg: Config, args: InitArgs) -> Result<()> {
    let mut a = vec!["init"];
    if let Some(p) = &args.path {
        a.push(p);
    }
    git::run_inherit(&a)?;
    println!("\nInitialized. Next: `cav repo connect <org>/<repo>` to wire it to CAVS Node.");
    Ok(())
}

#[derive(clap::Args)]
pub struct CloneArgs {
    /// The Git URL to clone.
    url: String,
    /// Target directory.
    dir: Option<String>,
    /// After cloning, connect to this CAVS repository as `<org>/<repo>`.
    #[arg(long, value_name = "ORG/REPO")]
    connect: Option<String>,
}

pub fn clone(cfg: Config, args: CloneArgs) -> Result<()> {
    let mut a = vec!["clone", args.url.as_str()];
    if let Some(d) = &args.dir {
        a.push(d);
    }
    git::run_inherit(&a)?;

    // Optionally wire the clone up to CAVS. We chdir into the clone so the
    // connect flow (which is repository-local) targets it.
    if let Some(reference) = args.connect {
        let dir = args
            .dir
            .clone()
            .unwrap_or_else(|| default_clone_dir(&args.url));
        std::env::set_current_dir(&dir).map_err(|e| anyhow::anyhow!("cannot enter {dir}: {e}"))?;
        repo::run(
            cfg,
            repo::RepoCommand::Connect(ConnectArgs::for_reference(reference)),
        )?;
    } else {
        println!("\nNext: `cd` into the clone and run `cav repo connect <org>/<repo>`.");
    }
    Ok(())
}

pub fn push(_cfg: Config, passthrough: Vec<String>) -> Result<()> {
    ensure_connected()?;
    let mut a = vec!["push"];
    a.extend(passthrough.iter().map(String::as_str));
    git::run_inherit(&a)
}

pub fn pull(_cfg: Config, passthrough: Vec<String>) -> Result<()> {
    ensure_connected()?;
    let mut a = vec!["pull"];
    a.extend(passthrough.iter().map(String::as_str));
    git::run_inherit(&a)
}

/// `sync` = pull then push, so a connected repo can be brought fully up to
/// date with the remote in one step.
pub fn sync(_cfg: Config, passthrough: Vec<String>) -> Result<()> {
    ensure_connected()?;
    let extra: Vec<&str> = passthrough.iter().map(String::as_str).collect();
    let mut pull = vec!["pull"];
    pull.extend(&extra);
    git::run_inherit(&pull)?;
    let mut push = vec!["push"];
    push.extend(&extra);
    git::run_inherit(&push)
}

/// Warn (don't block) when the repo isn't connected — plain git still works,
/// but large files won't route to CAVS without the LFS agent wired.
fn ensure_connected() -> Result<()> {
    if git::top_level().is_err() {
        bail!("not inside a git repository");
    }
    if git::get_config("cavs.repo-id")?.is_none() {
        eprintln!("warning: this repository is not connected to CAVS — run `cav repo connect <org>/<repo>` so large files route to the Hub");
    }
    Ok(())
}

/// Git's default clone directory: the last path segment of the URL, minus a
/// trailing ".git".
fn default_clone_dir(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or(trimmed);
    last.trim_end_matches(".git").to_string()
}
