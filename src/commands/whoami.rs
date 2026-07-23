//! `cav whoami` — show the authenticated identity and its organizations.

use crate::api::Client;
use crate::config::Config;
use crate::error::{err, Category};
use crate::output;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
struct WhoamiJson {
    identity: Option<String>,
    api: String,
    organizations: Vec<OrgJson>,
}

#[derive(Serialize)]
struct OrgJson {
    slug: String,
    name: String,
}

pub fn run(cfg: Config) -> Result<()> {
    if !cfg.is_logged_in() {
        return Err(err(
            Category::AuthRequired,
            "not logged in — run `cav login` first",
        ));
    }
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let me = client.me()?;

    if output::is_json() {
        let out = WhoamiJson {
            identity: me.user.as_ref().map(|u| u.label()),
            api: cfg.api_base.clone(),
            organizations: me
                .organizations
                .iter()
                .map(|o| OrgJson {
                    slug: o.slug.clone(),
                    name: o.name.clone(),
                })
                .collect(),
        };
        return output::emit_json(&out);
    }

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
