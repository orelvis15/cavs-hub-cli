//! `cav repo …` — repository-level operations.

use crate::api::Client;
use crate::commands::{index, install_lfs};
use crate::config::Config;
use crate::git;
use crate::hooks;
use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum RepoCommand {
    /// Connect the current Git repository to a CAVS Node repository.
    Connect(ConnectArgs),
    /// Upload the git index (commits, branches, tags, LFS file tree) to the Hub.
    Index(index::IndexArgs),
}

#[derive(clap::Args)]
pub struct ConnectArgs {
    /// The CAVS repository as `<org>/<repo>` (slugs, as shown in the dashboard).
    #[arg(value_name = "ORG/REPO")]
    reference: String,

    /// Do not wire up the CAVS LFS transfer agent (just set the LFS URL).
    #[arg(long)]
    skip_lfs: bool,

    /// Do not install the pre-push hook or upload the initial git index.
    #[arg(long)]
    skip_index: bool,

    /// Path to the `cavs-lfs-agent` binary (defaults to the one on PATH).
    #[arg(long, value_name = "PATH")]
    agent_path: Option<String>,
}

impl ConnectArgs {
    /// Build default connect args for a `<org>/<repo>` reference (used by
    /// `cav clone --connect`).
    pub fn for_reference(reference: String) -> Self {
        Self {
            reference,
            skip_lfs: false,
            skip_index: false,
            agent_path: None,
        }
    }
}

pub fn run(cfg: Config, command: RepoCommand) -> Result<()> {
    match command {
        RepoCommand::Connect(args) => connect(cfg, args),
        RepoCommand::Index(args) => index::run_index(cfg, args),
    }
}

fn connect(cfg: Config, args: ConnectArgs) -> Result<()> {
    if !cfg.is_logged_in() {
        bail!("not logged in — run `cav login` first");
    }

    let (org, repo) = args
        .reference
        .split_once('/')
        .ok_or_else(|| anyhow!("reference must be <org>/<repo>, got {:?}", args.reference))?;
    if org.is_empty() || repo.is_empty() {
        bail!("reference must be <org>/<repo>");
    }

    // Fail early if we are not in a repo, before hitting the network.
    let top = git::top_level()?;

    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let repos = client
        .list_repos(org)
        .with_context(|| format!("listing repositories for organization {org:?}"))?;
    let found = repos
        .into_iter()
        .find(|r| r.slug == repo)
        .ok_or_else(|| anyhow!("repository {org}/{repo} not found (or you lack access)"))?;

    let info = client
        .repo_connect(&found.id)
        .with_context(|| format!("fetching connection details for {org}/{repo}"))?;

    // Point git-lfs at the CAVS endpoint for this repository.
    git::set_config("lfs.url", &info.lfs_url)?;
    // The pre-push hook and `cav repo index` resolve the Hub repo from these.
    git::set_config("cavs.repo-id", &found.id)?;
    git::set_config("cavs.api-base", &cfg.api_base)?;

    let repo_ref = if info.repository_ref.is_empty() {
        format!("{org}/{repo}")
    } else {
        info.repository_ref.clone()
    };
    println!("Connected {top} to {repo_ref}");
    if !info.endpoint.is_empty() {
        println!("  endpoint: {}", info.endpoint);
    }
    println!("  lfs.url:  {}", info.lfs_url);

    if args.skip_lfs {
        println!("\nSkipped LFS agent setup (--skip-lfs). Run `cav install-lfs` when ready.");
    } else {
        let agent = install_lfs::wire(&args.agent_path)?;
        // Pin the agent's remote to the CAVS LFS URL. Without this the agent
        // falls back to the git remote announced by git-lfs (e.g. "origin"),
        // resolves it to the GitHub URL, and treats that as a local directory —
        // so large files never reach the Hub. Passing --remote here routes them
        // to CAVS explicitly.
        git::set_config(
            "lfs.customtransfer.cavs.args",
            &format!("--remote {}", info.lfs_url),
        )?;
        println!("  agent:    {agent}");
    }

    if args.skip_index {
        println!(
            "\nSkipped git-index setup (--skip-index). Run `cav repo index --full` when ready."
        );
    } else {
        // The pre-push hook keeps the Hub's Files/Commits/Branches view fresh on
        // every push; the initial `--full` upload backfills existing history
        // (this is the "import repository" flow).
        hooks::install_pre_push().context("installing the pre-push hook")?;
        println!("  hook:     pre-push (git index)");
        println!("\nUploading the initial git index (commits, branches, LFS files)…");
        if let Err(err) = index::run_index(cfg, index::IndexArgs::full_backfill()) {
            eprintln!(
                "warning: initial index failed ({err:#}); run `cav repo index --full` to retry"
            );
        }
    }

    println!("\nNext: commit your large files and run `git push`.");
    Ok(())
}
