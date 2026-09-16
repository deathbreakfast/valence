//! GitHub (or compatible) blob URL helpers for View source.

/// Optional overrides when resolving View source links.
#[derive(Debug, Clone, Default)]
pub struct SourceLinkConfig {
    /// Override default branch name (otherwise `"main"` then host default).
    pub branch: Option<String>,
}

/// Resolved link to a source file on the default branch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceLink {
    pub repository: String,
    pub path: String,
    pub line: u32,
    pub url: String,
}

impl SourceLink {
    /// Build a blob URL: `{repository}/blob/{branch}/{path}#L{line}`.
    ///
    /// `repository` should be a HTTPS GitHub-style repo root without trailing slash.
    /// `path` is repo-root-relative (no leading slash).
    #[must_use]
    pub fn github_blob(repository: &str, path: &str, line: u32, config: &SourceLinkConfig) -> Self {
        let branch = config.branch.as_deref().unwrap_or("main");
        let repo = repository.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        let url = format!("{repo}/blob/{branch}/{path}#L{line}");
        Self {
            repository: repo.to_string(),
            path: path.to_string(),
            line,
            url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_url_default_branch() {
        let link = SourceLink::github_blob(
            "https://github.com/unified-field-dev/valence",
            "valence-core/src/lib.rs",
            42,
            &SourceLinkConfig::default(),
        );
        assert_eq!(
            link.url,
            "https://github.com/unified-field-dev/valence/blob/main/valence-core/src/lib.rs#L42"
        );
    }
}
