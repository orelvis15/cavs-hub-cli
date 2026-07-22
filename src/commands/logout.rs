//! `cav logout` — drop the stored credentials.

use crate::config::Config;
use anyhow::Result;

pub fn run(mut cfg: Config) -> Result<()> {
    let was_logged_in = cfg.token.take().is_some();
    cfg.account = None;
    cfg.save()?;
    if was_logged_in {
        println!("Logged out of {}.", cfg.api_base);
    } else {
        println!("Not logged in.");
    }
    Ok(())
}
