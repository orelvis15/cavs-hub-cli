//! Hub read/write commands that talk to the control-plane REST API:
//! `artifacts`, `search`, `storage`, `snapshot`, `release`.

use crate::api::Client;
use crate::config::Config;
use crate::output;
use crate::util;
use anyhow::{Context, Result};
use serde::Serialize;

// --- cav artifacts ----------------------------------------------------------

#[derive(clap::Args)]
pub struct ArtifactsArgs {
    /// Organization slug (defaults to your only organization).
    #[arg(long)]
    org: Option<String>,
    /// Filter by type: model | dataset | media | archive | other.
    #[arg(long = "type", value_name = "TYPE")]
    kind: Option<String>,
    /// Filter by name substring.
    #[arg(long)]
    query: Option<String>,
}

pub fn artifacts(cfg: Config, args: ArtifactsArgs) -> Result<()> {
    util::require_login(&cfg)?;
    let org = util::resolve_org(&cfg, args.org)?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let list = client
        .list_artifacts(&org, args.kind.as_deref(), args.query.as_deref())
        .with_context(|| format!("listing artifacts for {org}"))?;

    if output::is_json() {
        #[derive(Serialize)]
        struct ArtifactJson<'a> {
            name: &'a str,
            #[serde(rename = "type")]
            kind: &'a str,
            size: i64,
            download_count: i64,
        }
        let out: Vec<_> = list
            .iter()
            .map(|a| ArtifactJson {
                name: &a.name,
                kind: &a.kind,
                size: a.size,
                download_count: a.download_count,
            })
            .collect();
        return output::emit_json(&out);
    }

    if list.is_empty() {
        println!("No artifacts found.");
        return Ok(());
    }
    println!(
        "{:<28} {:<10} {:>10} {:>10}",
        "NAME", "TYPE", "SIZE", "DOWNLOADS"
    );
    for a in &list {
        println!(
            "{:<28} {:<10} {:>10} {:>10}",
            truncate(&a.name, 28),
            a.kind,
            util::bytes(a.size),
            a.download_count
        );
    }
    println!("\n{} artifact(s).", list.len());
    Ok(())
}

// --- cav search -------------------------------------------------------------

#[derive(clap::Args)]
pub struct SearchArgs {
    /// The text to search for (repositories and commit messages).
    query: String,
}

pub fn search(cfg: Config, args: SearchArgs) -> Result<()> {
    util::require_login(&cfg)?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let res = client.search(&args.query).context("searching")?;

    if output::is_json() {
        #[derive(Serialize)]
        struct RepoJson<'a> {
            name: &'a str,
            slug: &'a str,
        }
        #[derive(Serialize)]
        struct CommitJson<'a> {
            sha: &'a str,
            message: &'a str,
        }
        #[derive(Serialize)]
        struct SearchJson<'a> {
            repositories: Vec<RepoJson<'a>>,
            commits: Vec<CommitJson<'a>>,
        }
        let out = SearchJson {
            repositories: res
                .repositories
                .iter()
                .map(|r| RepoJson {
                    name: &r.name,
                    slug: &r.slug,
                })
                .collect(),
            commits: res
                .commits
                .iter()
                .map(|c| CommitJson {
                    sha: &c.sha,
                    message: &c.message,
                })
                .collect(),
        };
        return output::emit_json(&out);
    }

    if res.repositories.is_empty() && res.commits.is_empty() {
        println!("No results.");
        return Ok(());
    }
    if !res.repositories.is_empty() {
        println!("Repositories");
        for r in &res.repositories {
            println!("  {} ({})", r.name, r.slug);
        }
    }
    if !res.commits.is_empty() {
        println!("\nCommits");
        for c in &res.commits {
            let sha = c.sha.chars().take(10).collect::<String>();
            println!("  {sha}  {}", first_line(&c.message));
        }
    }
    Ok(())
}

// --- cav storage ------------------------------------------------------------

#[derive(clap::Args)]
pub struct StorageArgs {
    /// Organization slug (defaults to your only organization).
    #[arg(long)]
    org: Option<String>,
}

pub fn storage(cfg: Config, args: StorageArgs) -> Result<()> {
    util::require_login(&cfg)?;
    let org = util::resolve_org(&cfg, args.org)?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let u = client
        .org_usage(&org)
        .with_context(|| format!("loading usage for {org}"))?;
    let saved = if u.usage.logical_storage_bytes > 0 {
        100.0 * (1.0 - u.usage.physical_storage_bytes as f64 / u.usage.logical_storage_bytes as f64)
    } else {
        0.0
    };

    if output::is_json() {
        #[derive(Serialize)]
        struct StorageJson<'a> {
            org: &'a str,
            logical_storage_bytes: i64,
            physical_storage_bytes: i64,
            saved_pct: f64,
            object_count: i64,
            download_bytes_month: i64,
            storage_used_pct: f64,
            egress_used_pct: f64,
        }
        return output::emit_json(&StorageJson {
            org: &org,
            logical_storage_bytes: u.usage.logical_storage_bytes,
            physical_storage_bytes: u.usage.physical_storage_bytes,
            saved_pct: saved,
            object_count: u.usage.object_count,
            download_bytes_month: u.usage.download_bytes_month,
            storage_used_pct: u.quota.storage_used_pct,
            egress_used_pct: u.quota.egress_used_pct,
        });
    }

    println!("Storage for {org}");
    println!(
        "  logical:   {}",
        util::bytes(u.usage.logical_storage_bytes)
    );
    println!(
        "  physical:  {}",
        util::bytes(u.usage.physical_storage_bytes)
    );
    println!("  saved:     {saved:.0}% (dedup + compression)");
    println!("  objects:   {}", u.usage.object_count);
    println!(
        "  egress MTD: {}",
        util::bytes(u.usage.download_bytes_month)
    );
    if u.quota.storage_used_pct > 0.0 {
        println!(
            "  quota:     {:.0}% storage · {:.0}% egress",
            u.quota.storage_used_pct, u.quota.egress_used_pct
        );
    }
    Ok(())
}

// --- cav snapshot -----------------------------------------------------------

#[derive(clap::Args)]
pub struct SnapshotArgs {
    /// Capture the snapshot at a specific ref (defaults to the default branch).
    #[arg(long)]
    reference: Option<String>,
}

pub fn snapshot(cfg: Config, args: SnapshotArgs) -> Result<()> {
    util::require_login(&cfg)?;
    let repo_id = util::connected_repo_id()?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    client
        .create_snapshot(&repo_id, args.reference.as_deref())
        .context("requesting snapshot")?;
    println!("Snapshot queued — it appears in the dashboard once captured.");
    Ok(())
}

// --- cav release ------------------------------------------------------------

#[derive(clap::Args)]
pub struct ReleaseArgs {}

pub fn release(cfg: Config, _args: ReleaseArgs) -> Result<()> {
    util::require_login(&cfg)?;
    let repo_id = util::connected_repo_id()?;
    let client = Client::new(&cfg.api_base, cfg.token.clone());
    let releases = client.list_releases(&repo_id).context("listing releases")?;

    if output::is_json() {
        #[derive(Serialize)]
        struct ReleaseJson<'a> {
            tag: &'a str,
            name: &'a str,
            lfs_file_count: i64,
            lfs_logical_bytes: i64,
        }
        let out: Vec<_> = releases
            .iter()
            .map(|r| ReleaseJson {
                tag: &r.tag,
                name: &r.name,
                lfs_file_count: r.lfs_file_count,
                lfs_logical_bytes: r.lfs_logical_bytes,
            })
            .collect();
        return output::emit_json(&out);
    }

    if releases.is_empty() {
        println!("No releases yet. Push a tag to create one.");
        return Ok(());
    }
    println!("{:<20} {:>8} {:>10}", "TAG", "FILES", "SIZE");
    for r in &releases {
        let label = if r.name.is_empty() { &r.tag } else { &r.name };
        println!(
            "{:<20} {:>8} {:>10}",
            truncate(label, 20),
            r.lfs_file_count,
            util::bytes(r.lfs_logical_bytes)
        );
    }
    Ok(())
}

// --- helpers ----------------------------------------------------------------

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}'); // …
    out
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}
