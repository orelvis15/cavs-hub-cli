//! Thin wrappers around the `git` executable for the config we need to write.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Absolute path of the current repository's working tree, erroring clearly
/// when the command is not run inside a Git repository.
pub fn top_level() -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("running `git` (is Git installed and on PATH?)")?;
    if !out.status.success() {
        bail!("not inside a Git repository — run this from within your repo");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Set a repository-local Git config key.
pub fn set_config(key: &str, value: &str) -> Result<()> {
    let out = Command::new("git")
        .args(["config", key, value])
        .output()
        .with_context(|| format!("running `git config {key}`"))?;
    if !out.status.success() {
        bail!(
            "`git config {key}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Locate an executable on `PATH`, honouring the `.exe` suffix on Windows.
pub fn find_on_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{bin}.exe"));
            if is_executable(&exe) {
                return Some(exe);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    p.is_file()
}
