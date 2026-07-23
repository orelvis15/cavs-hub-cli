//! End-to-end CLI tests exercising only the local validation and exit-code
//! paths — none of these require a running API. Network-touching commands are
//! driven only far enough to hit a *local* guard (bad oid, missing file,
//! missing login), which fails before any request is made.
//!
//! Login state is controlled by pointing `$XDG_CONFIG_HOME` at a temp dir and
//! (optionally) writing a `cav/config.toml` with a token. The daily update
//! check is disabled with `CAV_NO_UPDATE_CHECK` so tests stay offline.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

/// Stable exit codes (mirrors `src/error.rs`).
const EXIT_AUTH_REQUIRED: i32 = 10;
const EXIT_INVALID_PATH: i32 = 17;

/// A `cav` command with an isolated, offline environment. When `token` is set,
/// the config is written so the CLI is "logged in" (no network is performed to
/// validate it — that only happens at `cav login`).
fn cav(config_home: &Path, token: Option<&str>) -> Command {
    if let Some(tok) = token {
        let dir = config_home.join("cav");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("config.toml"),
            format!("api_base = \"http://127.0.0.1:9\"\ntoken = \"{tok}\"\n"),
        )
        .unwrap();
    }
    let mut cmd = Command::cargo_bin("cav").unwrap();
    cmd.env("XDG_CONFIG_HOME", config_home)
        .env("CAV_NO_UPDATE_CHECK", "1")
        .env_remove("CAVS_API");
    cmd
}

#[test]
fn help_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    cav(tmp.path(), None)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CAVS Node command-line client"));
}

#[test]
fn version_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    cav(tmp.path(), None)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn download_bad_oid_exits_invalid_path() {
    // Logged in (so we pass require_login), but the oid is malformed, which is
    // caught locally before any network call.
    let tmp = tempfile::tempdir().unwrap();
    cav(tmp.path(), Some("cavs_testtoken"))
        .args(["download", "not-a-real-oid"])
        .assert()
        .code(EXIT_INVALID_PATH)
        .stderr(predicate::str::contains("INVALID_PATH"))
        .stderr(predicate::str::contains("SHA-256 oid"));
}

#[test]
fn verify_missing_file_exits_invalid_path() {
    let tmp = tempfile::tempdir().unwrap();
    cav(tmp.path(), Some("cavs_testtoken"))
        .args(["verify", "/no/such/file/at/all.bin"])
        .assert()
        .code(EXIT_INVALID_PATH)
        .stderr(predicate::str::contains("INVALID_PATH"));
}

#[test]
fn command_without_login_exits_auth_required() {
    // Empty config dir -> no token -> AuthRequired before any network.
    let tmp = tempfile::tempdir().unwrap();
    cav(tmp.path(), None)
        .arg("whoami")
        .assert()
        .code(EXIT_AUTH_REQUIRED)
        .stderr(predicate::str::contains("AUTH_REQUIRED"))
        .stderr(predicate::str::contains("not logged in"));
}

#[test]
fn verify_without_login_exits_auth_required() {
    let tmp = tempfile::tempdir().unwrap();
    cav(tmp.path(), None)
        .args(["verify", "whatever.bin"])
        .assert()
        .code(EXIT_AUTH_REQUIRED)
        .stderr(predicate::str::contains("AUTH_REQUIRED"));
}

#[test]
fn status_json_is_valid_json() {
    // `status` is fully local (no network, no login needed).
    let tmp = tempfile::tempdir().unwrap();
    let out = cav(tmp.path(), None)
        .args(["status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).expect("stdout must be valid JSON");
    assert!(v.get("api").is_some(), "expected an `api` field: {v}");
    assert_eq!(v.get("logged_in").and_then(|b| b.as_bool()), Some(false));
}
