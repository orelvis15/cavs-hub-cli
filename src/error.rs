//! Stable error taxonomy and process exit codes.
//!
//! Commands surface failures as ordinary `anyhow` errors, but where a call site
//! knows *why* something failed it attaches a [`CliError`] carrying a
//! [`Category`]. `main` walks the error chain, and if it finds a category it
//! exits with that category's stable code and prints a machine-readable prefix
//! (e.g. `error[AUTH_REQUIRED]: …`). Uncategorized failures keep exit code 1.

use std::fmt;

/// Categories of failure the CLI can surface, each mapped to a stable exit
/// code so scripts and CI can branch on the reason a command failed rather than
/// parsing the human message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    AuthRequired,       // 10
    PermissionDenied,   // 11
    RepoNotConnected,   // 12
    OrgAmbiguous,       // 13
    NetworkUnavailable, // 14
    ApiError,           // 15
    RateLimited,        // 16
    InvalidPath,        // 17
    ChecksumMismatch,   // 18
    MissingObject,      // 19
    ConfigInvalid,      // 20
    VersionUnsupported, // 21
}

impl Category {
    /// The stable process exit code for this category.
    pub fn exit_code(self) -> i32 {
        match self {
            Category::AuthRequired => 10,
            Category::PermissionDenied => 11,
            Category::RepoNotConnected => 12,
            Category::OrgAmbiguous => 13,
            Category::NetworkUnavailable => 14,
            Category::ApiError => 15,
            Category::RateLimited => 16,
            Category::InvalidPath => 17,
            Category::ChecksumMismatch => 18,
            Category::MissingObject => 19,
            Category::ConfigInvalid => 20,
            Category::VersionUnsupported => 21,
        }
    }

    /// Short machine-readable slug printed alongside errors (e.g. `AUTH_REQUIRED`).
    pub fn code(self) -> &'static str {
        match self {
            Category::AuthRequired => "AUTH_REQUIRED",
            Category::PermissionDenied => "PERMISSION_DENIED",
            Category::RepoNotConnected => "REPO_NOT_CONNECTED",
            Category::OrgAmbiguous => "ORG_AMBIGUOUS",
            Category::NetworkUnavailable => "NETWORK_UNAVAILABLE",
            Category::ApiError => "API_ERROR",
            Category::RateLimited => "RATE_LIMITED",
            Category::InvalidPath => "INVALID_PATH",
            Category::ChecksumMismatch => "CHECKSUM_MISMATCH",
            Category::MissingObject => "MISSING_OBJECT",
            Category::ConfigInvalid => "CONFIG_INVALID",
            Category::VersionUnsupported => "VERSION_UNSUPPORTED",
        }
    }
}

/// A categorized CLI error: a human message plus a stable [`Category`]. Wrapped
/// into `anyhow` chains so `main` can downcast to it and pick the exit code
/// without every call site knowing about exit codes.
#[derive(Debug)]
pub struct CliError {
    pub category: Category,
    pub message: String,
}

impl CliError {
    pub fn new(category: Category, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Build a categorized error ready to `return Err(..)` from an `anyhow::Result`.
pub fn err(category: Category, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CliError::new(category, message))
}

/// Find the most specific [`Category`] in an error chain, if any call site
/// attached one.
pub fn find_category(err: &anyhow::Error) -> Option<Category> {
    for cause in err.chain() {
        if let Some(c) = cause.downcast_ref::<CliError>() {
            return Some(c.category);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_stable_and_distinct() {
        let all = [
            Category::AuthRequired,
            Category::PermissionDenied,
            Category::RepoNotConnected,
            Category::OrgAmbiguous,
            Category::NetworkUnavailable,
            Category::ApiError,
            Category::RateLimited,
            Category::InvalidPath,
            Category::ChecksumMismatch,
            Category::MissingObject,
            Category::ConfigInvalid,
            Category::VersionUnsupported,
        ];
        // The exact mapping is a stable contract (plan §21.3).
        assert_eq!(Category::AuthRequired.exit_code(), 10);
        assert_eq!(Category::PermissionDenied.exit_code(), 11);
        assert_eq!(Category::RepoNotConnected.exit_code(), 12);
        assert_eq!(Category::OrgAmbiguous.exit_code(), 13);
        assert_eq!(Category::NetworkUnavailable.exit_code(), 14);
        assert_eq!(Category::ApiError.exit_code(), 15);
        assert_eq!(Category::RateLimited.exit_code(), 16);
        assert_eq!(Category::InvalidPath.exit_code(), 17);
        assert_eq!(Category::ChecksumMismatch.exit_code(), 18);
        assert_eq!(Category::MissingObject.exit_code(), 19);
        assert_eq!(Category::ConfigInvalid.exit_code(), 20);
        assert_eq!(Category::VersionUnsupported.exit_code(), 21);

        // No two categories collide, and none reuses the generic code 1.
        let mut codes: Vec<i32> = all.iter().map(|c| c.exit_code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), all.len());
        assert!(all.iter().all(|c| c.exit_code() != 1));
    }

    #[test]
    fn category_is_recovered_through_anyhow_context() {
        let e = err(Category::ChecksumMismatch, "boom")
            .context("while downloading")
            .context("outer");
        assert_eq!(find_category(&e), Some(Category::ChecksumMismatch));
    }

    #[test]
    fn uncategorized_errors_have_no_category() {
        let e = anyhow::anyhow!("plain failure");
        assert_eq!(find_category(&e), None);
    }
}
