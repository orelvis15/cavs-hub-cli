//! `cav doctor` — diagnose the local environment and Hub connectivity.
//!
//! Runs a series of best-effort checks and prints a pass/warn/fail line for
//! each, then exits non-zero if any hard check failed. Nothing here mutates
//! state, so it is safe to run anytime a push or pull misbehaves.

use crate::api::Client;
use crate::config::Config;
use crate::output;
use crate::{git, hooks};
use anyhow::Result;
use serde::Serialize;

enum Status {
    Ok,
    Warn,
    Fail,
}

impl Status {
    fn glyph(&self) -> &'static str {
        match self {
            Status::Ok => "\u{2713}",   // ✓
            Status::Warn => "\u{26A0}", // ⚠
            Status::Fail => "\u{2717}", // ✗
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }
}

#[derive(Serialize)]
struct CheckJson {
    section: String,
    check: String,
    status: String,
    detail: String,
}

struct Report {
    failed: bool,
    json: bool,
    section: String,
    checks: Vec<CheckJson>,
}

impl Report {
    fn new(json: bool) -> Self {
        Self {
            failed: false,
            json,
            section: String::new(),
            checks: Vec::new(),
        }
    }
    fn section(&mut self, name: &str) {
        self.section = name.to_string();
        if !self.json {
            if self.checks.is_empty() {
                println!("{name}");
            } else {
                println!("\n{name}");
            }
        }
    }
    fn line(&mut self, status: Status, label: &str, detail: impl AsRef<str>) {
        if matches!(status, Status::Fail) {
            self.failed = true;
        }
        let detail = detail.as_ref();
        if !self.json {
            if detail.is_empty() {
                println!("  {} {label}", status.glyph());
            } else {
                println!("  {} {label}: {detail}", status.glyph());
            }
        }
        self.checks.push(CheckJson {
            section: self.section.clone(),
            check: label.to_string(),
            status: status.label().to_string(),
            detail: detail.to_string(),
        });
    }
}

pub fn run(cfg: Config) -> Result<()> {
    let mut r = Report::new(output::is_json());
    r.section("Environment");

    // git present?
    match git::find_on_path("git") {
        Some(p) => r.line(Status::Ok, "git", p.display().to_string()),
        None => r.line(Status::Fail, "git", "not found on PATH"),
    }
    // git-lfs present?
    match git::find_on_path("git-lfs") {
        Some(p) => r.line(Status::Ok, "git-lfs", p.display().to_string()),
        None => r.line(Status::Warn, "git-lfs", "not found (needed for push/pull)"),
    }
    // CAVS transfer agent present?
    match git::find_on_path("cavs-lfs-agent") {
        Some(p) => r.line(Status::Ok, "cavs-lfs-agent", p.display().to_string()),
        None => r.line(
            Status::Warn,
            "cavs-lfs-agent",
            "not found (run `cav install-lfs`)",
        ),
    }

    r.section("Authentication");
    if cfg.is_logged_in() {
        let who = cfg
            .account
            .clone()
            .unwrap_or_else(|| "authenticated".into());
        r.line(Status::Ok, "token", format!("stored ({who})"));
    } else {
        r.line(Status::Fail, "token", "not logged in (run `cav login`)");
    }

    r.section("Connectivity");
    r.line(Status::Ok, "api", cfg.api_base.clone());
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    match client.healthz() {
        Ok(_) => r.line(Status::Ok, "reachable", ""),
        Err(e) => r.line(Status::Fail, "reachable", format!("{e:#}")),
    }
    // Token actually accepted?
    if cfg.is_logged_in() {
        match client.me() {
            Ok(me) => {
                let orgs = me.organizations.len();
                r.line(
                    Status::Ok,
                    "identity",
                    format!("{orgs} organization(s) visible"),
                );
            }
            Err(e) => r.line(Status::Fail, "identity", format!("token rejected: {e:#}")),
        }
    }

    // Repo wiring, when inside a connected repository.
    if git::top_level().is_ok() {
        r.section("Repository");
        match git::get_config("cavs.repo-id")? {
            Some(id) => r.line(Status::Ok, "connected", id),
            None => r.line(
                Status::Warn,
                "connected",
                "no CAVS repo wired (run `cav repo connect`)",
            ),
        }
        match git::get_config("lfs.url")? {
            Some(url) => r.line(Status::Ok, "lfs.url", url),
            None => r.line(Status::Warn, "lfs.url", "unset"),
        }
        match hooks::pre_push_installed() {
            Ok(true) => r.line(Status::Ok, "pre-push hook", "installed"),
            Ok(false) => r.line(Status::Warn, "pre-push hook", "missing"),
            Err(_) => r.line(Status::Warn, "pre-push hook", "unknown"),
        }
    }

    if r.json {
        #[derive(Serialize)]
        struct DoctorJson {
            ok: bool,
            checks: Vec<CheckJson>,
        }
        output::emit_json(&DoctorJson {
            ok: !r.failed,
            checks: r.checks,
        })?;
        if r.failed {
            return Err(crate::error::err(
                crate::error::Category::ConfigInvalid,
                "doctor found problems",
            ));
        }
        return Ok(());
    }

    println!();
    if r.failed {
        return Err(crate::error::err(
            crate::error::Category::ConfigInvalid,
            "doctor found problems (see the \u{2717} lines above)",
        ));
    }
    println!("All checks passed.");
    Ok(())
}
