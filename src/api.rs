//! Minimal HTTP client for the CAVS Node control-plane API.
//!
//! Only the handful of read endpoints the CLI needs are modelled. Responses
//! are decoded loosely (unknown fields ignored, missing fields defaulted) so
//! the client keeps working as the API grows new fields.

use crate::error::{err as cli_err, Category};
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Seek, Write};
use std::time::Duration;

/// Small number of attempts for transient failures (429 / 5xx / network).
const MAX_ATTEMPTS: u32 = 4;

pub struct Client {
    base: String,
    token: Option<String>,
    /// Control-plane JSON calls: short, aggressive overall timeout.
    agent: ureq::Agent,
    /// Bulk object transfer (presigned PUT/GET): generous read timeout so a
    /// legitimate large upload/download is never aborted by a control-plane cap.
    transfer_agent: ureq::Agent,
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
        let ua = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
        // Control plane: keep the whole request on a short leash — these are
        // small JSON round-trips and hanging on them helps nobody.
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .user_agent(ua)
            .build();
        // Transfers can legitimately run for minutes. Bound the connect and
        // per-read stalls, but do NOT put a global cap on the whole transfer.
        let transfer_agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(300))
            .user_agent(ua)
            .build();
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            token,
            agent,
            transfer_agent,
        }
    }

    /// Send a request with retry on transient failures (429 / 5xx / network).
    ///
    /// `send` is a closure so the request can be rebuilt (and any body re-sent)
    /// on each attempt. Backoff uses [`backoff_delay`] and, for 429, honours a
    /// `Retry-After` header when present. Status errors are mapped to a
    /// [`Category`] via [`status_to_error`]; transport errors become
    /// `NetworkUnavailable`.
    fn retry_send<F>(&self, ctx: &str, mut send: F) -> Result<ureq::Response>
    where
        F: FnMut() -> std::result::Result<ureq::Response, ureq::Error>,
    {
        for attempt in 0..MAX_ATTEMPTS {
            match send() {
                Ok(resp) => return Ok(resp),
                Err(ureq::Error::Status(code, resp))
                    if (code == 429 || code >= 500) && attempt + 1 < MAX_ATTEMPTS =>
                {
                    let wait = if code == 429 {
                        parse_retry_after(resp.header("Retry-After"))
                            .unwrap_or_else(|| backoff_delay(attempt))
                    } else {
                        backoff_delay(attempt)
                    };
                    let _ = resp.into_string();
                    std::thread::sleep(wait);
                }
                Err(ureq::Error::Status(code, resp)) => return Err(status_to_error(code, resp)),
                Err(ureq::Error::Transport(_)) if attempt + 1 < MAX_ATTEMPTS => {
                    std::thread::sleep(backoff_delay(attempt));
                }
                Err(e) => {
                    return Err(cli_err(
                        Category::NetworkUnavailable,
                        format!("network error {ctx}: {e}"),
                    ))
                }
            }
        }
        unreachable!("retry loop returns or bails within MAX_ATTEMPTS")
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base, path)
    }

    // The closures below return `Result<_, ureq::Error>`; ureq's error enum is
    // large, but boxing it here would only obscure the retry logic, so we allow
    // the lint at these call sites.
    #[allow(clippy::result_large_err)]
    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        // GETs are retried on transient failures too (429 / 5xx / network).
        let resp = self.retry_send(&url, || {
            let mut req = self.agent.get(&url);
            if let Some(token) = &self.token {
                req = req.set("Authorization", &format!("Bearer {token}"));
            }
            req.call()
        })?;
        resp.into_json::<T>().context("decoding API response")
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

    #[allow(clippy::result_large_err)]
    fn post_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        // Retry on rate limiting and transient server errors so a long initial
        // backfill survives hiccups (see `retry_send` for the policy).
        let url = self.url(path);
        let resp = self.retry_send(&url, || {
            let mut req = self.agent.post(&url);
            if let Some(token) = &self.token {
                req = req.set("Authorization", &format!("Bearer {token}"));
            }
            req.send_json(body)
        })?;
        resp.into_json::<T>().context("decoding API response")
    }

    // --- Git index ingest ---------------------------------------------------

    /// What the Hub already knows about this repository's refs.
    pub fn index_state(&self, repo_id: &str) -> Result<IndexState> {
        self.get(&format!("/repositories/{repo_id}/index/state"))
    }

    pub fn create_index_session(&self, repo_id: &str) -> Result<IndexSession> {
        #[derive(Serialize)]
        struct Body {
            client_version: String,
        }
        self.post_json(
            &format!("/repositories/{repo_id}/index/sessions"),
            &Body {
                client_version: env!("CARGO_PKG_VERSION").to_string(),
            },
        )
    }

    pub fn push_commits(
        &self,
        repo_id: &str,
        session: &str,
        commits: &[IndexCommit],
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            commits: &'a [IndexCommit],
        }
        let _: serde_json::Value = self.post_json(
            &format!("/repositories/{repo_id}/index/sessions/{session}/commits"),
            &Body { commits },
        )?;
        Ok(())
    }

    pub fn push_tree_page(
        &self,
        repo_id: &str,
        session: &str,
        reference: &str,
        entries: &[IndexTreeEntry],
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(rename = "ref")]
            reference: &'a str,
            entries: &'a [IndexTreeEntry],
        }
        let _: serde_json::Value = self.post_json(
            &format!("/repositories/{repo_id}/index/sessions/{session}/tree"),
            &Body { reference, entries },
        )?;
        Ok(())
    }

    pub fn finalize_index(&self, repo_id: &str, session: &str, body: &IndexFinalize) -> Result<()> {
        let _: serde_json::Value = self.post_json(
            &format!("/repositories/{repo_id}/index/sessions/{session}/finalize"),
            body,
        )?;
        Ok(())
    }

    // --- Hub read/write endpoints (artifacts, search, storage, snapshots) ---

    /// Global search scoped to the caller's organizations.
    pub fn search(&self, query: &str) -> Result<SearchResults> {
        let q = urlencode(query);
        self.get(&format!("/search?q={q}"))
    }

    /// The org-wide artifact registry, optionally filtered by type/query.
    pub fn list_artifacts(
        &self,
        org: &str,
        kind: Option<&str>,
        query: Option<&str>,
    ) -> Result<Vec<Artifact>> {
        let mut path = format!("/organizations/{org}/artifacts?limit=100");
        if let Some(k) = kind {
            path.push_str(&format!("&type={}", urlencode(k)));
        }
        if let Some(q) = query {
            path.push_str(&format!("&q={}", urlencode(q)));
        }
        #[derive(Deserialize)]
        struct Wrap {
            #[serde(default)]
            artifacts: Vec<Artifact>,
        }
        Ok(self.get::<Wrap>(&path)?.artifacts)
    }

    /// Organization usage summary + plan quota (for `cav storage`).
    pub fn org_usage(&self, org: &str) -> Result<UsageResp> {
        self.get(&format!("/organizations/{org}/usage"))
    }

    /// Queue a storage snapshot for a repository (optionally at a ref).
    pub fn create_snapshot(&self, repo_id: &str, reference: Option<&str>) -> Result<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
            reference: Option<&'a str>,
        }
        let _: serde_json::Value = self.post_json(
            &format!("/repositories/{repo_id}/snapshots"),
            &Body { reference },
        )?;
        Ok(())
    }

    /// Releases of a repository (newest first).
    pub fn list_releases(&self, repo_id: &str) -> Result<Vec<Release>> {
        #[derive(Deserialize)]
        struct Wrap {
            #[serde(default)]
            releases: Vec<Release>,
        }
        Ok(self
            .get::<Wrap>(&format!("/repositories/{repo_id}/releases"))?
            .releases)
    }

    // --- Direct object transfer (cav upload / download / verify) ------------

    /// Open an upload session; returns the session id.
    pub fn create_upload_session(&self, repo_id: &str, expected: usize) -> Result<String> {
        #[derive(Serialize)]
        struct Body {
            expected_objects: usize,
        }
        #[derive(Deserialize)]
        struct Wrap {
            session: SessionId,
        }
        #[derive(Deserialize)]
        struct SessionId {
            id: String,
        }
        let wrap: Wrap = self.post_json(
            &format!("/repositories/{repo_id}/uploads"),
            &Body {
                expected_objects: expected,
            },
        )?;
        Ok(wrap.session.id)
    }

    /// Authorize a batch of objects, returning a presigned PUT URL per oid.
    pub fn authorize_objects(
        &self,
        repo_id: &str,
        session: &str,
        objects: &[(String, i64)],
    ) -> Result<Vec<AuthorizedUpload>> {
        #[derive(Serialize)]
        struct Obj<'a> {
            oid: &'a str,
            size: i64,
        }
        #[derive(Serialize)]
        struct Body<'a> {
            objects: Vec<Obj<'a>>,
        }
        #[derive(Deserialize)]
        struct Wrap {
            #[serde(default)]
            objects: Vec<AuthorizedUpload>,
        }
        let body = Body {
            objects: objects
                .iter()
                .map(|(o, s)| Obj { oid: o, size: *s })
                .collect(),
        };
        Ok(self
            .post_json::<_, Wrap>(
                &format!("/repositories/{repo_id}/uploads/{session}/objects"),
                &body,
            )?
            .objects)
    }

    /// Mark an object uploaded (records its bytes against the session).
    pub fn complete_object(
        &self,
        repo_id: &str,
        session: &str,
        oid: &str,
        size: i64,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Body {
            size: i64,
        }
        let _: serde_json::Value = self.post_json(
            &format!("/repositories/{repo_id}/uploads/{session}/objects/{oid}/complete"),
            &Body { size },
        )?;
        Ok(())
    }

    /// Finalize an upload session (bumps the repository generation).
    pub fn finalize_upload(&self, repo_id: &str, session: &str) -> Result<()> {
        let _: serde_json::Value = self.post_json(
            &format!("/repositories/{repo_id}/uploads/{session}/finalize"),
            &serde_json::json!({}),
        )?;
        Ok(())
    }

    /// Authorize downloads, returning a presigned GET URL per oid.
    pub fn authorize_download(
        &self,
        repo_id: &str,
        oids: &[String],
    ) -> Result<Vec<AuthorizedDownload>> {
        #[derive(Serialize)]
        struct Body<'a> {
            oids: &'a [String],
        }
        #[derive(Deserialize)]
        struct Wrap {
            #[serde(default)]
            objects: Vec<AuthorizedDownload>,
        }
        Ok(self
            .post_json::<_, Wrap>(
                &format!("/repositories/{repo_id}/downloads/authorize"),
                &Body { oids },
            )?
            .objects)
    }

    /// Stream a PUT body to a presigned URL from a seekable reader (outside
    /// /api/v1), without buffering the object in memory.
    ///
    /// The reader is rewound before each attempt so transient 5xx / network
    /// failures can be retried a small number of times. `len` is sent as the
    /// `Content-Length` so object stores that require it are satisfied.
    pub fn put_presigned_reader<R: Read + Seek>(
        &self,
        url: &str,
        mut reader: R,
        len: u64,
    ) -> Result<()> {
        for attempt in 0..MAX_ATTEMPTS {
            reader.rewind().map_err(|e| {
                cli_err(Category::InvalidPath, format!("rewinding upload body: {e}"))
            })?;
            match self
                .transfer_agent
                .put(url)
                .set("Content-Length", &len.to_string())
                .send(&mut reader)
            {
                Ok(_) => return Ok(()),
                Err(ureq::Error::Status(code, resp))
                    if code >= 500 && attempt + 1 < MAX_ATTEMPTS =>
                {
                    let _ = resp.into_string();
                    std::thread::sleep(backoff_delay(attempt));
                }
                Err(ureq::Error::Status(code, resp)) => return Err(status_to_error(code, resp)),
                Err(ureq::Error::Transport(_)) if attempt + 1 < MAX_ATTEMPTS => {
                    std::thread::sleep(backoff_delay(attempt));
                }
                Err(e) => {
                    return Err(cli_err(
                        Category::NetworkUnavailable,
                        format!("uploading object bytes: {e}"),
                    ))
                }
            }
        }
        unreachable!("retry loop returns or bails within MAX_ATTEMPTS")
    }

    /// Stream a GET body from a presigned URL into `writer` (outside /api/v1),
    /// returning the number of bytes copied. The response body is never held in
    /// memory. The connect/response phase is retried on transient failures; a
    /// mid-stream read failure is not retried (bytes are already partially
    /// consumed) and surfaces as `NetworkUnavailable`.
    #[allow(clippy::result_large_err)]
    pub fn get_presigned_to_writer<W: Write>(&self, url: &str, writer: &mut W) -> Result<u64> {
        let resp = self.retry_send(url, || self.transfer_agent.get(url).call())?;
        let mut reader = resp.into_reader();
        io::copy(&mut reader, writer).map_err(|e| {
            cli_err(
                Category::NetworkUnavailable,
                format!("reading object bytes: {e}"),
            )
        })
    }

    /// Lightweight connectivity probe used by `cav doctor`.
    pub fn healthz(&self) -> Result<()> {
        // /healthz is public and lives outside /api/v1.
        let url = format!("{}/healthz", self.base);
        match self.agent.get(&url).call() {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(code, resp)) => Err(status_to_error(code, resp)),
            Err(e) => Err(cli_err(
                Category::NetworkUnavailable,
                format!("reaching the API: {e}"),
            )),
        }
    }
}

/// Map an HTTP status code from any endpoint to a categorized error, keeping the
/// server's human message. This turns the old flat "API returned HTTP {code}"
/// into something scripts can branch on by exit code.
fn status_to_error(code: u16, resp: ureq::Response) -> anyhow::Error {
    let category = match code {
        401 => Category::AuthRequired,
        403 => Category::PermissionDenied,
        426 => Category::VersionUnsupported, // Upgrade Required
        429 => Category::RateLimited,
        _ => Category::ApiError,
    };
    cli_err(
        category,
        format!("API returned HTTP {code}: {}", extract_error(resp)),
    )
}

/// Exponential backoff with deterministic pseudo-jitter.
///
/// We avoid pulling in an RNG crate, so the "jitter" is derived from the attempt
/// number (`attempt * 137ms`) and added to a doubling base delay. It is enough
/// to desynchronize retries from a lockstep exponential curve without a new
/// dependency; it is intentionally not cryptographically random.
fn backoff_delay(attempt: u32) -> Duration {
    let base = 500u64.saturating_mul(1u64 << attempt.min(6));
    let jitter = 137u64.saturating_mul(attempt as u64 + 1);
    Duration::from_millis(base + jitter)
}

/// Parse a `Retry-After` header expressed in seconds, capped so a hostile or
/// buggy value can't make the CLI sleep for minutes. Only the delta-seconds
/// form is supported (HTTP-date form returns `None`).
fn parse_retry_after(header: Option<&str>) -> Option<Duration> {
    let secs: u64 = header?.trim().parse().ok()?;
    Some(Duration::from_secs(secs.min(30)))
}

/// Percent-encode a query-string value (RFC 3986 unreserved kept as-is).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// --- Git index payloads -----------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IndexState {
    #[serde(default)]
    pub refs: Vec<IndexRefState>,
}

#[derive(Debug, Deserialize)]
pub struct IndexRefState {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub head_sha: String,
}

#[derive(Debug, Deserialize)]
pub struct IndexSession {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct IndexArtifact {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub oid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prev_oid: String,
    pub size: i64,
    pub prev_size: i64,
}

#[derive(Debug, Serialize)]
pub struct IndexCommit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    pub committed_at: String,
    pub message: String,
    pub artifacts: Vec<IndexArtifact>,
    pub files_changed: usize,
}

#[derive(Debug, Serialize)]
pub struct IndexTreeEntry {
    pub path: String,
    pub oid: String,
    pub size: i64,
}

#[derive(Debug, Serialize)]
pub struct IndexRefHead {
    pub name: String,
    pub kind: String,
    pub head_sha: String,
    pub head_committed_at: String,
}

#[derive(Debug, Serialize)]
pub struct IndexTag {
    pub tag: String,
    pub target_sha: String,
    pub message: String,
    pub tagged_at: String,
}

#[derive(Debug, Serialize)]
pub struct IndexFinalize {
    pub refs: Vec<IndexRefHead>,
    pub deleted_refs: Vec<String>,
    pub tags: Vec<IndexTag>,
    pub stats: IndexStats,
}

#[derive(Debug, Serialize)]
pub struct IndexStats {
    pub commits: usize,
    pub files: usize,
}

// --- Hub read payloads ------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct Artifact {
    // Kept for completeness / future `cav artifacts --oid` output; not printed
    // in the current table view.
    #[serde(default)]
    #[allow(dead_code)]
    pub oid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub download_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct SearchResults {
    #[serde(default)]
    pub repositories: Vec<SearchRepo>,
    #[serde(default)]
    pub commits: Vec<SearchCommit>,
}

#[derive(Debug, Deserialize)]
pub struct SearchRepo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchCommit {
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct UsageResp {
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub quota: Quota,
}

#[derive(Debug, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub physical_storage_bytes: i64,
    #[serde(default)]
    pub logical_storage_bytes: i64,
    #[serde(default)]
    pub download_bytes_month: i64,
    #[serde(default)]
    pub object_count: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct Quota {
    #[serde(default)]
    pub storage_used_pct: f64,
    #[serde(default)]
    pub egress_used_pct: f64,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizedUpload {
    #[serde(default)]
    pub oid: String,
    #[serde(default)]
    pub upload_url: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizedDownload {
    #[serde(default)]
    pub oid: String,
    // Server-reported size; the CLI validates against the actual bytes, so this
    // is informational only.
    #[serde(default)]
    #[allow(dead_code)]
    pub size: i64,
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Release {
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub lfs_file_count: i64,
    #[serde(default)]
    pub lfs_logical_bytes: i64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_parses_seconds_and_caps() {
        assert_eq!(parse_retry_after(Some("5")), Some(Duration::from_secs(5)));
        assert_eq!(
            parse_retry_after(Some("  10 ")),
            Some(Duration::from_secs(10))
        );
        // Capped at 30s regardless of how large the header claims.
        assert_eq!(
            parse_retry_after(Some("9999")),
            Some(Duration::from_secs(30))
        );
        // HTTP-date form and garbage are not supported.
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn backoff_grows_and_carries_attempt_jitter() {
        // Base doubles each attempt; jitter is a deterministic function of the
        // attempt so the sequence is monotonic and reproducible.
        assert_eq!(backoff_delay(0), Duration::from_millis(500 + 137));
        assert_eq!(backoff_delay(1), Duration::from_millis(1000 + 274));
        assert_eq!(backoff_delay(2), Duration::from_millis(2000 + 411));
        assert!(backoff_delay(3) > backoff_delay(2));
    }
}
