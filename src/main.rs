//! `cav` — the CAVS Node command-line client.
//!
//! This is the tool users install with
//! `curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | sh`.
//! It talks to the CAVS
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
mod error;
mod git;
mod hooks;
mod lfs;
mod output;
mod update;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cav", version, about = "CAVS Node command-line client")]
struct Cli {
    /// Override the API base URL for this invocation (also read from
    /// $CAVS_API, then the stored config, then the built-in default).
    #[arg(long, global = true, value_name = "URL")]
    api: Option<String>,

    /// Emit machine-readable JSON on stdout (for the data commands).
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-essential progress/info output (errors and results remain).
    #[arg(long, short, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authenticate with a CAVS Node access token.
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
    /// Inspect or edit the persisted CLI configuration.
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommand,
    },
    /// Diagnose the local environment and Hub connectivity.
    Doctor,
    /// Initialize a new Git repository (then `cav repo connect`).
    Init(commands::gitwrap::InitArgs),
    /// Clone a Git repository (optionally connecting it to CAVS).
    Clone(commands::gitwrap::CloneArgs),
    /// Push the current repository (routes large files through CAVS).
    Push {
        /// Extra arguments passed straight to `git push`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Pull the current repository (fetches CAVS-stored large files).
    Pull {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Sync the current repository: pull then push.
    Sync {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// List artifacts across an organization's repositories.
    Artifacts(commands::hub::ArtifactsArgs),
    /// Search repositories and commits.
    Search(commands::hub::SearchArgs),
    /// Show an organization's storage usage.
    Storage(commands::hub::StorageArgs),
    /// Queue a storage snapshot for the connected repository.
    Snapshot(commands::hub::SnapshotArgs),
    /// List the connected repository's releases.
    Release(commands::hub::ReleaseArgs),
    /// Upload files directly to the connected repository (no git needed).
    Upload(commands::transfer::UploadArgs),
    /// Download an object by its SHA-256 oid.
    Download(commands::transfer::DownloadArgs),
    /// Verify local files exist on the Hub (by content hash).
    Verify(commands::transfer::VerifyArgs),
    /// Update cav to the latest published release.
    Update(update::Args),
    /// Git hook entrypoints (installed by `cav repo connect`).
    #[command(hide = true)]
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
}

#[derive(Subcommand)]
enum HookCommand {
    /// pre-push: index the refs being pushed (best-effort, never blocks).
    #[command(name = "pre-push")]
    PrePush {
        /// Remote name (as git passes to the hook).
        remote: String,
        /// Remote URL (as git passes to the hook).
        url: String,
    },
}

fn main() {
    if let Err(e) = run() {
        // If a call site attached a category, use its stable exit code and print
        // a machine-readable prefix; otherwise fall back to generic failure (1).
        match error::find_category(&e) {
            Some(cat) => {
                eprintln!("error[{}]: {e:#}", cat.code());
                std::process::exit(cat.exit_code());
            }
            None => {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    output::init(cli.json, cli.quiet);

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
        Command::Config { command } => commands::config::run(cfg, command),
        Command::Doctor => commands::doctor::run(cfg),
        Command::Init(args) => commands::gitwrap::init(cfg, args),
        Command::Clone(args) => commands::gitwrap::clone(cfg, args),
        Command::Push { args } => commands::gitwrap::push(cfg, args),
        Command::Pull { args } => commands::gitwrap::pull(cfg, args),
        Command::Sync { args } => commands::gitwrap::sync(cfg, args),
        Command::Artifacts(args) => commands::hub::artifacts(cfg, args),
        Command::Search(args) => commands::hub::search(cfg, args),
        Command::Storage(args) => commands::hub::storage(cfg, args),
        Command::Snapshot(args) => commands::hub::snapshot(cfg, args),
        Command::Release(args) => commands::hub::release(cfg, args),
        Command::Upload(args) => commands::transfer::upload(cfg, args),
        Command::Download(args) => commands::transfer::download(cfg, args),
        Command::Verify(args) => commands::transfer::verify(cfg, args),
        Command::Update(args) => update::run(args),
        Command::Hook { command } => match command {
            HookCommand::PrePush { remote, url } => {
                commands::index::run_hook_pre_push(cfg, remote, url)
            }
        },
    }
}

fn normalize_base(raw: String) -> String {
    raw.trim().trim_end_matches('/').to_string()
}
