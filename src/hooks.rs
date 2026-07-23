//! Git hook management: the pre-push wrapper installed by `cav repo connect`.
//!
//! git-lfs owns `.git/hooks/pre-push` after `git lfs install --local`, and the
//! hook's stdin (the pushed-refs list) can only be read once. The wrapper tees
//! stdin to a temp file, runs `git lfs pre-push` first (a failure there must
//! still abort the push), then feeds the same input to `cav hook pre-push`
//! best-effort — the git-index upload never blocks a push.

use crate::git;
use anyhow::{Context, Result};
use std::path::PathBuf;

const MARKER: &str = "# >>> cavs pre-push hook >>>";
const BACKUP_NAME: &str = "pre-push.cavs-bak";

fn wrapper_script(chain_backup: bool) -> String {
    let chained = if chain_backup {
        format!(
            "hookdir=\"$(dirname \"$0\")\"\n\
             if [ -x \"$hookdir/{BACKUP_NAME}\" ]; then\n\
             \t\"$hookdir/{BACKUP_NAME}\" \"$@\" <\"$tmp\" || {{ status=$?; rm -f \"$tmp\"; exit $status; }}\n\
             fi\n"
        )
    } else {
        String::new()
    };
    format!(
        "#!/bin/sh\n\
         {MARKER}\n\
         # Managed by `cav repo connect`. Re-run it to repair this hook.\n\
         tmp=\"$(mktemp)\" || exit 0\n\
         cat >\"$tmp\"\n\
         if command -v git-lfs >/dev/null 2>&1; then\n\
         \tgit lfs pre-push \"$@\" <\"$tmp\" || {{ status=$?; rm -f \"$tmp\"; exit $status; }}\n\
         fi\n\
         {chained}\
         if command -v cav >/dev/null 2>&1; then\n\
         \tcav hook pre-push \"$@\" <\"$tmp\" || true\n\
         fi\n\
         rm -f \"$tmp\"\n\
         exit 0\n"
    )
}

/// Is this hook body one we can safely replace without chaining? Covers our
/// own wrapper (idempotent reinstall) and the stock git-lfs hook.
fn replaceable(body: &str) -> bool {
    body.contains(MARKER) || is_stock_lfs_hook(body)
}

fn is_stock_lfs_hook(body: &str) -> bool {
    body.contains("git lfs pre-push") && body.lines().filter(|l| !l.trim().is_empty()).count() <= 5
}

fn hook_path() -> Result<PathBuf> {
    Ok(git::git_dir()?.join("hooks").join("pre-push"))
}

/// Install (or repair) the pre-push wrapper. A foreign existing hook is moved
/// to `pre-push.cavs-bak` and chained so it keeps running.
pub fn install_pre_push() -> Result<()> {
    let path = hook_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut chain = path
        .parent()
        .map(|d| d.join(BACKUP_NAME).exists())
        .unwrap_or(false);
    if !existing.is_empty() && !replaceable(&existing) {
        let backup = path.with_file_name(BACKUP_NAME);
        std::fs::rename(&path, &backup)
            .with_context(|| format!("backing up existing hook to {}", backup.display()))?;
        make_executable(&backup)?;
        chain = true;
    }

    std::fs::write(&path, wrapper_script(chain))
        .with_context(|| format!("writing {}", path.display()))?;
    make_executable(&path)?;
    Ok(())
}

/// Reports the hook state for `cav status`: Ok(true) when our wrapper is in
/// place, Ok(false) when missing or foreign.
pub fn pre_push_installed() -> Result<bool> {
    let path = hook_path()?;
    Ok(std::fs::read_to_string(path)
        .map(|body| body.contains(MARKER))
        .unwrap_or(false))
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("marking {} executable", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
