//! Persisted CLI configuration: the API base URL and the stored access token.
//!
//! The file lives at `$XDG_CONFIG_HOME/cav/config.toml` (falling back to
//! `~/.config/cav/config.toml`) and is written with `0600` permissions on
//! Unix because it holds a bearer token.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The production control plane. Override with `--api`, `$CAVS_API`, or by
/// editing the stored config (e.g. `http://localhost:8080` for local dev).
pub const DEFAULT_API_BASE: &str = "https://api.cavscloud.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_api_base")]
    pub api_base: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Cached identity label (email or display name) for friendly output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

fn default_api_base() -> String {
    DEFAULT_API_BASE.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_base: default_api_base(),
            token: None,
            account: None,
        }
    }
}

impl Config {
    /// Absolute path to the config file (honouring `$XDG_CONFIG_HOME`).
    pub fn path() -> Result<PathBuf> {
        let base = match std::env::var("XDG_CONFIG_HOME") {
            Ok(x) if !x.is_empty() => PathBuf::from(x),
            _ => home_config_dir()?,
        };
        Ok(base.join("cav").join("config.toml"))
    }

    /// Load the config, returning defaults if the file does not exist yet.
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        match std::fs::read_to_string(&path) {
            Ok(raw) => toml::from_str(&raw).with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Persist the config, creating the directory and restricting permissions.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let raw = toml::to_string_pretty(self).context("serializing config")?;
        std::fs::write(&path, raw).with_context(|| format!("writing {}", path.display()))?;
        restrict_permissions(&path)?;
        Ok(())
    }

    pub fn is_logged_in(&self) -> bool {
        self.token
            .as_deref()
            .map(|t| !t.is_empty())
            .unwrap_or(false)
    }
}

fn home_config_dir() -> Result<PathBuf> {
    let home =
        std::env::var("HOME").context("HOME is not set; cannot locate the config directory")?;
    Ok(PathBuf::from(home).join(".config"))
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("restricting permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
