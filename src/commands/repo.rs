//! `cav repo …` — repository-level operations.

use crate::api::Client;
use crate::commands::install_lfs;
use crate::config::Config;
use crate::git;
use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum RepoCommand {
    /// Connect the current Git repository to a CAVS Hub repository.
    Connect(ConnectArgs),
}

#[derive(clap::Args)]
pub struct ConnectArgs {
    /// The CAVS repository as `<org>/<repo>` (slugs, as shown in the dashboard).
    #[arg(value_name = "ORG/REPO")]
    reference: String,

    /// Do not wire up the CAVS LFS transfer agent (just set the LFS URL).
    #[arg(long)]
    skip_lfs: bool,

    /// Path to the `cavs-lfs-agent` binary (defaults to the one on PATH).
    #[arg(long, value_name = "PATH")]
    agent_path: Option<String>,
}

pub fn run(cfg: Config, command: RepoCommand) -> Result<()> {
    match command {
        RepoCommand::Connect(args) => connect(cfg, args),
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
        println!("  agent:    {agent}");
    }

    println!("\nNext: commit your large files and run `git push`.");
    Ok(())
}
