//! `cav status` — print where the config lives and the current login state.
//! Purely local: it makes no network calls.

use crate::config::Config;
use crate::output;
use crate::{git, hooks};
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
struct StatusJson {
    config: String,
    api: String,
    logged_in: bool,
    account: Option<String>,
    repo_id: Option<String>,
    pre_push_hook: Option<bool>,
}

pub fn run(cfg: Config) -> Result<()> {
    let config_path = Config::path()?.display().to_string();
    let logged_in = cfg.is_logged_in();

    // Repo wiring is only meaningful inside a git working tree.
    let (repo_id, hook) = if git::top_level().is_ok() {
        let repo_id = git::get_config("cavs.repo-id")?.filter(|s| !s.is_empty());
        let hook = hooks::pre_push_installed().ok();
        (repo_id, hook)
    } else {
        (None, None)
    };

    if output::is_json() {
        return output::emit_json(&StatusJson {
            config: config_path,
            api: cfg.api_base.clone(),
            logged_in,
            account: cfg.account.clone(),
            repo_id,
            pre_push_hook: hook,
        });
    }

    println!("config: {config_path}");
    println!("api:    {}", cfg.api_base);
    let login = if logged_in {
        match &cfg.account {
            Some(a) => format!("yes ({a})"),
            None => "yes".to_string(),
        }
    } else {
        "no (run `cav login`)".to_string()
    };
    println!("login:  {login}");

    // When run inside a connected repo, report the git-index wiring health.
    if git::top_level().is_ok() {
        match &repo_id {
            Some(id) => println!("repo:   connected ({id})"),
            None => println!("repo:   not connected (run `cav repo connect <org>/<repo>`)"),
        }
        let hook = match hook {
            Some(true) => "installed",
            Some(false) => "missing (re-run `cav repo connect` to repair)",
            None => "unknown",
        };
        println!("hook:   pre-push {hook}");
    }
    Ok(())
}
