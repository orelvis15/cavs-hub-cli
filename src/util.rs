//! Small shared helpers for the command implementations.

use crate::api::Client;
use crate::config::Config;
use crate::error::{err, Category};
use crate::git;
use anyhow::Result;

/// Human-readable byte size (binary units), e.g. 1536 -> "1.5 KiB".
pub fn bytes(n: i64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
}

/// Resolve the CAVS repository id for the current working directory, set by
/// `cav repo connect` in git config as `cavs.repo-id`.
pub fn connected_repo_id() -> Result<String> {
    if git::top_level().is_err() {
        return Err(err(
            Category::RepoNotConnected,
            "not inside a git repository",
        ));
    }
    match git::get_config("cavs.repo-id")? {
        Some(id) if !id.is_empty() => Ok(id),
        _ => Err(err(
            Category::RepoNotConnected,
            "this repository is not connected — run `cav repo connect <org>/<repo>` first",
        )),
    }
}

/// Resolve the organization slug to operate on: an explicit value wins;
/// otherwise fall back to the caller's only organization, erroring when the
/// choice is ambiguous.
pub fn resolve_org(cfg: &Config, explicit: Option<String>) -> Result<String> {
    if let Some(o) = explicit {
        if !o.is_empty() {
            return Ok(o);
        }
    }
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let orgs = client.me()?.organizations;
    match orgs.len() {
        0 => Err(err(
            Category::OrgAmbiguous,
            "you don't belong to any organization yet",
        )),
        1 => Ok(orgs[0].slug.clone()),
        _ => {
            let names: Vec<_> = orgs.iter().map(|o| o.slug.clone()).collect();
            Err(err(
                Category::OrgAmbiguous,
                format!(
                    "you belong to several organizations; pass --org <slug> (one of: {})",
                    names.join(", ")
                ),
            ))
        }
    }
}

/// Require a stored token, with a consistent, categorized error.
pub fn require_login(cfg: &Config) -> Result<()> {
    if !cfg.is_logged_in() {
        return Err(err(
            Category::AuthRequired,
            "not logged in — run `cav login` first",
        ));
    }
    Ok(())
}
