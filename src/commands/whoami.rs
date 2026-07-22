//! `cav whoami` — show the authenticated identity and its organizations.

use crate::api::Client;
use crate::config::Config;
use anyhow::{bail, Result};

pub fn run(cfg: Config) -> Result<()> {
    if !cfg.is_logged_in() {
        bail!("not logged in — run `cav login` first");
    }
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let me = client.me()?;

    match &me.user {
        Some(u) => println!("{}", u.label()),
        None => println!("(token principal — no user context)"),
    }
    println!("api: {}", cfg.api_base);
    for org in &me.organizations {
        println!("org: {} ({})", org.slug, org.name);
    }
    Ok(())
}
