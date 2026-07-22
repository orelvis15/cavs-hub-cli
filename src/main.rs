//! `cav` — the CAVS Hub command-line client.
//!
//! This is the tool users install with
//! `curl -fsSL https://cavscloud.com/install.sh | sh`. It talks to the CAVS
//! Hub control plane (the Go API in the `cavshub` repo) over HTTPS and wires a
//! local Git repository up to the CAVS custom Git LFS transfer agent
//! (`cavs-lfs-agent`, from the `cavs-oss` repo).
//!
//! It is deliberately a *thin* client: authentication is a CAVS access token
//! (prefixed `cavs_`, created in the dashboard), the heavy lifting of
//! content-defined chunking and dedup happens in the LFS agent, and the local
//! packaging commands live in the separate OSS `cavs` binary (cavs-oss).

mod api;
mod commands;
mod config;
mod git;
mod update;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cav", version, about = "CAVS Hub command-line client")]
struct Cli {
    /// Override the API base URL for this invocation (also read from
    /// $CAVS_API, then the stored config, then the built-in default).
    #[arg(long, global = true, value_name = "URL")]
    api: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authenticate with a CAVS Hub access token.
    Login(commands::login::Args),
    /// Remove the stored credentials.
    Logout,
    /// Show the authenticated identity and organizations.
    Whoami,
    /// Repository operations.
    Repo {
        #[command(subcommand)]
        command: commands::repo::RepoCommand,
    },
    /// Configure the current Git repository to use the CAVS LFS transfer agent.
    #[command(name = "install-lfs")]
    InstallLfs(commands::install_lfs::Args),
    /// Print the CLI configuration and login state.
    Status,
    /// Update cav to the latest published release.
    Update(update::Args),
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let mut cfg = config::Config::load()?;
    // Precedence for the API base: --api flag, then $CAVS_API, then whatever is
    // already stored in the config (default applied on load).
    if let Some(api) = cli.api {
        cfg.api_base = normalize_base(api);
    } else if let Ok(api) = std::env::var("CAVS_API") {
        if !api.is_empty() {
            cfg.api_base = normalize_base(api);
        }
    }

    // Best-effort, throttled to once a day: warn on stderr if a newer release
    // exists. Skipped for `update` (which checks explicitly).
    if !matches!(cli.command, Command::Update(_)) {
        update::check_and_warn();
    }

    match cli.command {
        Command::Login(args) => commands::login::run(cfg, args),
        Command::Logout => commands::logout::run(cfg),
        Command::Whoami => commands::whoami::run(cfg),
        Command::Repo { command } => commands::repo::run(cfg, command),
        Command::InstallLfs(args) => commands::install_lfs::run(cfg, args),
        Command::Status => commands::status::run(cfg),
        Command::Update(args) => update::run(args),
    }
}

fn normalize_base(raw: String) -> String {
    raw.trim().trim_end_matches('/').to_string()
}
