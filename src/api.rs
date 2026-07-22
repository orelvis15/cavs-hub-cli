//! Minimal HTTP client for the CAVS Hub control-plane API.
//!
//! Only the handful of read endpoints the CLI needs are modelled. Responses
//! are decoded loosely (unknown fields ignored, missing fields defaulted) so
//! the client keeps working as the API grows new fields.

use anyhow::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::time::Duration;

pub struct Client {
    base: String,
    token: Option<String>,
    agent: ureq::Agent,
}

#[derive(Debug, Deserialize)]
pub struct Org {
    // Present in the API model; the CLI resolves orgs by slug, so `id` is kept
    // for completeness but not currently read.
    #[serde(default)]
    #[allow(dead_code)]
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Repo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct User {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub display_name: String,
}

impl User {
    /// Best label to show for this user.
    pub fn label(&self) -> String {
        if !self.email.is_empty() {
            self.email.clone()
        } else {
            self.display_name.clone()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Me {
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub organizations: Vec<Org>,
}

/// Response of `GET /repositories/{id}/connect`.
#[derive(Debug, Deserialize)]
pub struct ConnectInfo {
    #[serde(default)]
    pub repository_ref: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub lfs_url: String,
}

impl Client {
    pub fn new(base: impl Into<String>, token: Option<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build();
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            token,
            agent,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base, path)
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut req = self.agent.get(&self.url(path));
        if let Some(token) = &self.token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        match req.call() {
            Ok(resp) => resp.into_json::<T>().context("decoding API response"),
            Err(ureq::Error::Status(code, resp)) => {
                bail!("API returned HTTP {code}: {}", extract_error(resp))
            }
            Err(e) => Err(anyhow!(e)).with_context(|| format!("requesting {}", self.url(path))),
        }
    }

    /// The authenticated identity plus the organizations it can see.
    pub fn me(&self) -> Result<Me> {
        self.get("/users/me")
    }

    /// Repositories in an organization (the `org` may be a slug or a UUID).
    pub fn list_repos(&self, org: &str) -> Result<Vec<Repo>> {
        #[derive(Deserialize)]
        struct Wrap {
            #[serde(default)]
            repositories: Vec<Repo>,
        }
        Ok(self
            .get::<Wrap>(&format!("/organizations/{org}/repositories"))?
            .repositories)
    }

    /// Connection details (endpoint + LFS URL) for a repository UUID.
    pub fn repo_connect(&self, repo_id: &str) -> Result<ConnectInfo> {
        self.get(&format!("/repositories/{repo_id}/connect"))
    }
}

/// Pull a human-readable message out of the API's `{"error":{"message":...}}`
/// envelope, falling back to the raw body.
fn extract_error(resp: ureq::Response) -> String {
    let body = resp.into_string().unwrap_or_default();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return msg.to_string();
        }
    }
    if body.trim().is_empty() {
        "(empty response body)".to_string()
    } else {
        body
    }
}
