//! `cav config` — inspect and edit the persisted CLI configuration.
//! Purely local: it reads and writes `~/.config/cav/config.toml`.

use crate::config::Config;
use anyhow::{bail, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Print all configuration values.
    List,
    /// Print a single value (`api` or `account`).
    Get { key: String },
    /// Set a value (`api` is the only writable key; use `cav login` for tokens).
    Set { key: String, value: String },
}

pub fn run(mut cfg: Config, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::List => {
            println!("api     = {}", cfg.api_base);
            println!("account = {}", cfg.account.as_deref().unwrap_or("(none)"));
            println!(
                "token   = {}",
                if cfg.is_logged_in() {
                    "(set)"
                } else {
                    "(none)"
                }
            );
            println!("\nfile: {}", Config::path()?.display());
            Ok(())
        }
        ConfigCommand::Get { key } => {
            let value = match key.as_str() {
                "api" | "api_base" => cfg.api_base.clone(),
                "account" => cfg.account.clone().unwrap_or_default(),
                other => bail!("unknown config key {other:?} (try: api, account)"),
            };
            println!("{value}");
            Ok(())
        }
        ConfigCommand::Set { key, value } => {
            match key.as_str() {
                "api" | "api_base" => {
                    cfg.api_base = value.trim().trim_end_matches('/').to_string();
                }
                "token" => bail!("set the token via `cav login`, not `cav config set`"),
                other => {
                    bail!("unknown or read-only config key {other:?} (only `api` is writable)")
                }
            }
            cfg.save()?;
            println!("Updated {key}.");
            Ok(())
        }
    }
}
