//! `cav status` — print where the config lives and the current login state.
//! Purely local: it makes no network calls.

use crate::config::Config;
use anyhow::Result;

pub fn run(cfg: Config) -> Result<()> {
    println!("config: {}", Config::path()?.display());
    println!("api:    {}", cfg.api_base);
    let login = if cfg.is_logged_in() {
        match &cfg.account {
            Some(a) => format!("yes ({a})"),
            None => "yes".to_string(),
        }
    } else {
        "no (run `cav login`)".to_string()
    };
    println!("login:  {login}");
    Ok(())
}
