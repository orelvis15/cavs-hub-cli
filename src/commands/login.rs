//! `cav login` — store and validate a CAVS Node access token.

use crate::api::Client;
use crate::config::Config;
use anyhow::{Context, Result};
use std::io::{self, Write};

#[derive(clap::Args)]
pub struct Args {
    /// The access token (prefixed `cavs_`). If omitted, you are prompted for
    /// it. Prefer the prompt or `--token-stdin` so the secret stays out of
    /// your shell history.
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,

    /// Read the token from stdin (e.g. `cat token.txt | cav login --token-stdin`).
    #[arg(long, conflicts_with = "token")]
    token_stdin: bool,
}

pub fn run(mut cfg: Config, args: Args) -> Result<()> {
    let token = resolve_token(&args)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("no token provided");
    }
    if !token.starts_with("cavs_") {
        eprintln!("warning: CAVS access tokens normally start with \"cavs_\" — continuing anyway");
    }

    // Validate the token against the API before persisting it.
    let client = Client::new(&cfg.api_base, Some(token.clone()));
    let me = client
        .me()
        .with_context(|| format!("validating the token against {}", cfg.api_base))?;

    cfg.token = Some(token);
    cfg.account = me.user.as_ref().map(|u| u.label());
    cfg.save()?;

    let who = cfg
        .account
        .clone()
        .unwrap_or_else(|| "(unknown)".to_string());
    println!("Logged in as {who} on {}", cfg.api_base);
    if me.organizations.is_empty() {
        println!("(no organizations yet — create one in the dashboard)");
    } else {
        println!("Organizations:");
        for org in &me.organizations {
            println!("  - {} ({})", org.slug, org.name);
        }
    }
    Ok(())
}

fn resolve_token(args: &Args) -> Result<String> {
    if let Some(t) = &args.token {
        return Ok(t.clone());
    }
    if args.token_stdin {
        let mut s = String::new();
        io::stdin()
            .read_line(&mut s)
            .context("reading token from stdin")?;
        return Ok(s);
    }
    prompt_token()
}

fn prompt_token() -> Result<String> {
    eprint!("Paste a CAVS access token (dashboard → Settings → Tokens): ");
    io::stderr().flush().ok();
    let mut s = String::new();
    io::stdin()
        .read_line(&mut s)
        .context("reading token from the prompt")?;
    Ok(s)
}
