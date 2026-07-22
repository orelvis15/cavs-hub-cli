//! `cav install-lfs` — point Git's LFS machinery at the CAVS transfer agent.
//!
//! CAVS ships a *standalone* custom transfer agent (`cavs-lfs-agent`). This
//! writes the repository-local Git config that tells git-lfs to hand all LFS
//! transfers to that agent, so only content-defined-chunked, deduplicated
//! bytes travel on push/pull.

use crate::config::Config;
use crate::git;
use anyhow::{anyhow, Result};

/// The name git-lfs uses for our custom transfer agent in `git config`.
const AGENT_NAME: &str = "cavs";
/// The binary that implements the agent protocol.
const AGENT_BIN: &str = "cavs-lfs-agent";

#[derive(clap::Args)]
pub struct Args {
    /// Path to the `cavs-lfs-agent` binary. Defaults to the copy found on PATH
    /// (installed alongside `cav` by install.sh).
    #[arg(long, value_name = "PATH")]
    agent_path: Option<String>,
}

pub fn run(_cfg: Config, args: Args) -> Result<()> {
    let top = git::top_level()?;
    let agent = wire(&args.agent_path)?;
    println!("Configured the CAVS LFS transfer agent for {top}");
    println!("  agent: {agent}");
    println!("git-lfs will now route transfers through CAVS. Next: `git push`.");
    Ok(())
}

/// Write the git-lfs custom-transfer-agent config and return the resolved agent
/// path. Shared by `install-lfs` and `repo connect`.
pub fn wire(agent_path: &Option<String>) -> Result<String> {
    // Ensure we are in a repo before touching config.
    git::top_level()?;

    let agent = match agent_path {
        Some(p) => p.clone(),
        None => git::find_on_path(AGENT_BIN)
            .ok_or_else(|| {
                anyhow!(
                    "`{AGENT_BIN}` not found on PATH.\n\
                     Install the CAVS tools (curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | sh)\n\
                     or pass --agent-path /path/to/{AGENT_BIN}"
                )
            })?
            .to_string_lossy()
            .to_string(),
    };

    git::set_config("lfs.standalonetransferagent", AGENT_NAME)?;
    git::set_config(&format!("lfs.customtransfer.{AGENT_NAME}.path"), &agent)?;
    // The agent processes one object per invocation; concurrency is handled by
    // git-lfs spawning it, so keep the per-agent concurrent flag off.
    git::set_config(
        &format!("lfs.customtransfer.{AGENT_NAME}.concurrent"),
        "false",
    )?;
    Ok(agent)
}
