//! Self-update (`cav update`) and the once-a-day "new version available" check.
//!
//! Both talk to the GitHub Releases API of this repository. The release
//! workflow publishes one asset per platform named `cav-<target-triple>` (with
//! a `.exe` suffix on Windows); `BUILD_TARGET` (set by build.rs) tells us which
//! one matches the running binary.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `owner/repo` used to build the GitHub API and download URLs.
const GITHUB_REPO: &str = "orelvis15/cavs-hub-cli";

/// How often the background check contacts GitHub (24h).
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

#[derive(clap::Args)]
pub struct Args {
    /// Only report whether a newer version exists; do not download anything.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
}

impl Release {
    /// The release version without the leading `v` (e.g. `0.1.0`).
    fn version(&self) -> String {
        self.tag_name.trim_start_matches('v').to_string()
    }
}

/// The version this binary was built as.
fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Asset name for the running platform, matching the release workflow.
fn asset_name() -> String {
    let target = env!("BUILD_TARGET");
    if cfg!(windows) {
        format!("cav-{target}.exe")
    } else {
        format!("cav-{target}")
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(timeout)
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
}

/// Fetch the latest release from the GitHub API.
fn latest_release(timeout: Duration) -> Result<Release> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let mut req = agent(timeout)
        .get(&url)
        .set("Accept", "application/vnd.github+json");
    // Optional token raises the unauthenticated rate limit; entirely optional.
    if let Some(tok) = github_token() {
        req = req.set("Authorization", &format!("Bearer {tok}"));
    }
    match req.call() {
        Ok(resp) => resp
            .into_json::<Release>()
            .context("decoding GitHub release JSON"),
        Err(ureq::Error::Status(404, _)) => {
            bail!("no published releases yet for {GITHUB_REPO}")
        }
        Err(ureq::Error::Status(code, _)) => bail!("GitHub API returned HTTP {code}"),
        Err(e) => Err(anyhow!(e)).context("contacting the GitHub API"),
    }
}

fn github_token() -> Option<String> {
    std::env::var("CAV_GITHUB_TOKEN")
        .or_else(|_| std::env::var("GITHUB_TOKEN"))
        .ok()
        .filter(|t| !t.is_empty())
}

/// Compare two dotted versions numerically. Returns true if `latest` > `current`.
/// Pre-release/build metadata is ignored (compared on the numeric core only).
fn is_newer(latest: &str, current: &str) -> bool {
    parse(latest) > parse(current)
}

fn parse(v: &str) -> (u64, u64, u64) {
    let core = v.trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut it = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

// --- `cav update` -----------------------------------------------------------

pub fn run(args: Args) -> Result<()> {
    let current = current_version();
    let release = latest_release(Duration::from_secs(30))?;
    let latest = release.version();

    if latest.is_empty() {
        bail!("could not determine the latest version");
    }
    if !is_newer(&latest, current) {
        println!("cav is up to date (v{current}).");
        return Ok(());
    }

    println!("A new version is available: v{latest} (you have v{current}).");
    if args.check {
        println!("Run `cav update` to install it.");
        return Ok(());
    }

    let want = asset_name();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == want)
        .ok_or_else(|| {
            anyhow!(
                "release v{latest} has no asset named {want} for this platform.\n\
                 Download manually from https://github.com/{GITHUB_REPO}/releases"
            )
        })?;

    println!("Downloading {}...", asset.name);
    self_replace(&asset.browser_download_url)?;
    println!("Updated cav to v{latest}.");
    Ok(())
}

/// Download `url` and atomically replace the currently running executable.
fn self_replace(url: &str) -> Result<()> {
    let current_exe = std::env::current_exe().context("locating the running executable")?;
    let dir = current_exe
        .parent()
        .ok_or_else(|| anyhow!("cannot determine the install directory"))?;

    let resp = agent(Duration::from_secs(120))
        .get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .context("reading the downloaded binary")?;
    if bytes.is_empty() {
        bail!("downloaded an empty file");
    }

    let tmp = dir.join(format!(".cav-update-{}.tmp", std::process::id()));
    write_binary(&tmp, &bytes).map_err(|e| permission_hint(dir, e))?;

    let result = swap_in_place(&current_exe, &tmp);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(|e| permission_hint(dir, e))
}

#[cfg(unix)]
fn write_binary(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o755)
        .open(path)?;
    std::io::Write::write_all(&mut f, bytes)
}

#[cfg(not(unix))]
fn write_binary(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(windows)]
fn swap_in_place(current: &std::path::Path, tmp: &std::path::Path) -> std::io::Result<()> {
    // A running .exe cannot be overwritten on Windows, but it can be renamed.
    let old = current.with_extension("old.exe");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(current, &old)?;
    std::fs::rename(tmp, current)?;
    Ok(())
}

#[cfg(not(windows))]
fn swap_in_place(current: &std::path::Path, tmp: &std::path::Path) -> std::io::Result<()> {
    // On Unix a rename over the running binary replaces the file the next exec
    // uses, without disturbing this process.
    std::fs::rename(tmp, current)
}

fn permission_hint(dir: &std::path::Path, e: std::io::Error) -> anyhow::Error {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        anyhow!(
            "cannot write to {} (permission denied).\n\
             Re-run with elevated privileges, or reinstall:\n\
             curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | sh",
            dir.display()
        )
    } else {
        anyhow!(e)
    }
}

// --- daily "new version available" check ------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct CheckState {
    #[serde(default)]
    last_check: u64,
    #[serde(default)]
    latest_known: Option<String>,
}

fn state_path() -> Option<PathBuf> {
    crate::config::Config::path()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("update_check.toml")))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Best-effort: at most once per day, ask GitHub for the latest version; if it
/// is newer than the running binary, print a warning to stderr. Network and IO
/// errors are swallowed so this never blocks or breaks a command. Opt out with
/// `CAV_NO_UPDATE_CHECK`.
pub fn check_and_warn() {
    if std::env::var_os("CAV_NO_UPDATE_CHECK").is_some() {
        return;
    }
    let Some(path) = state_path() else { return };
    let mut state: CheckState = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();

    let now = now_secs();
    if state.last_check == 0 || now.saturating_sub(state.last_check) >= CHECK_INTERVAL_SECS {
        // Throttle regardless of outcome so a failing network doesn't hammer.
        state.last_check = now;
        if let Ok(rel) = latest_release(Duration::from_secs(3)) {
            let v = rel.version();
            if !v.is_empty() {
                state.latest_known = Some(v);
            }
        }
        let _ = save_state(&path, &state);
    }

    if let Some(latest) = &state.latest_known {
        if is_newer(latest, current_version()) {
            eprintln!(
                "\x1b[33mwarning:\x1b[0m a new version of cav is available: v{latest} \
                 (you have v{}). Run `cav update`.",
                current_version()
            );
        }
    }
}

fn save_state(path: &std::path::Path, state: &CheckState) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, toml::to_string_pretty(state)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn version_ignores_prefix_and_prerelease() {
        assert_eq!(parse("v1.2.3"), (1, 2, 3));
        assert_eq!(parse("1.2.3-rc.1"), (1, 2, 3));
        assert!(!is_newer("v0.1.0", "0.1.0"));
    }
}
