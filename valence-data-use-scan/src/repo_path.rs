//! Repository-relative source paths for catalog rows and View source links.
//!
//! Packages arrive from the host workspace, sibling checkouts, cargo's git
//! checkouts, and the registry. Rows must never carry the build machine's
//! absolute path, so every file is expressed relative to the root of the
//! repository that owns its package.

use std::path::{Path, PathBuf};

/// Where a package's files are rooted for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoRoot {
    /// Repository checkout root on disk (`.git` or cargo's `.cargo-ok` marker).
    Checkout(PathBuf),
    /// Registry package: files live under `path_in_vcs` inside the original repo.
    PathInVcs {
        /// Package directory on disk.
        package_dir: PathBuf,
        /// `path_in_vcs` from `.cargo_vcs_info.json` (empty for a repo-root crate).
        path_in_vcs: String,
    },
    /// No repository marker found: paths fall back to package-relative.
    Package(PathBuf),
}

impl RepoRoot {
    /// Detect the repository root that owns `package_dir`.
    pub fn detect(package_dir: &Path) -> Self {
        if let Some(path_in_vcs) = read_path_in_vcs(package_dir) {
            return Self::PathInVcs {
                package_dir: package_dir.to_path_buf(),
                path_in_vcs,
            };
        }
        for dir in package_dir.ancestors() {
            if dir.join(".git").exists() || dir.join(".cargo-ok").exists() {
                return Self::Checkout(dir.to_path_buf());
            }
        }
        Self::Package(package_dir.to_path_buf())
    }

    /// `file` relative to the repository root, `/`-separated and never absolute.
    ///
    /// Files outside the detected root (which should not happen for files under
    /// the package directory) fall back to their file name.
    pub fn relative(&self, file: &Path) -> String {
        let rel = match self {
            Self::Checkout(root) | Self::Package(root) => {
                file.strip_prefix(root).ok().map(Path::to_path_buf)
            }
            Self::PathInVcs {
                package_dir,
                path_in_vcs,
            } => file.strip_prefix(package_dir).ok().map(|inner| {
                if path_in_vcs.is_empty() {
                    inner.to_path_buf()
                } else {
                    Path::new(path_in_vcs).join(inner)
                }
            }),
        };
        let rel = rel.unwrap_or_else(|| file.file_name().map(PathBuf::from).unwrap_or_default());
        to_slash(&rel)
    }
}

fn read_path_in_vcs(package_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(package_dir.join(".cargo_vcs_info.json")).ok()?;
    let marker = "\"path_in_vcs\"";
    let start = raw.find(marker)? + marker.len();
    let rest = raw[start..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].trim_matches('/').to_string())
}

fn to_slash(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn repo_relative_from_cargo_ok_root() {
        let dir = tempdir().unwrap();
        let checkout = dir.path().join("checkouts/counter-app-1a2b/abc1234");
        let pkg = checkout.join("counter-app-worker");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(checkout.join(".cargo-ok"), "").unwrap();

        let root = RepoRoot::detect(&pkg);
        assert_eq!(root, RepoRoot::Checkout(checkout));
        assert_eq!(
            root.relative(&pkg.join("src/session.rs")),
            "counter-app-worker/src/session.rs"
        );
    }

    #[test]
    fn repo_relative_from_git_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("valence-uf-app");
        let pkg = repo.join("probe/data-use-probe-product");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();

        let root = RepoRoot::detect(&pkg);
        assert_eq!(
            root.relative(&pkg.join("src/lib.rs")),
            "probe/data-use-probe-product/src/lib.rs"
        );
    }

    #[test]
    fn repo_relative_from_cargo_vcs_info_path_in_vcs() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("registry/src/index/uf-lepton-0.1.0");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join(".cargo_vcs_info.json"),
            "{\n  \"git\": {\n    \"sha1\": \"abc\"\n  },\n  \"path_in_vcs\": \"lepton\"\n}\n",
        )
        .unwrap();

        let root = RepoRoot::detect(&pkg);
        assert_eq!(
            root.relative(&pkg.join("src/auth.rs")),
            "lepton/src/auth.rs"
        );

        fs::write(pkg.join(".cargo_vcs_info.json"), r#"{"path_in_vcs": ""}"#).unwrap();
        assert_eq!(
            RepoRoot::detect(&pkg).relative(&pkg.join("src/auth.rs")),
            "src/auth.rs"
        );
    }

    #[test]
    fn fallback_is_package_relative_never_absolute() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("loose/product");
        fs::create_dir_all(pkg.join("src")).unwrap();

        let root = RepoRoot::detect(&pkg);
        let rel = root.relative(&pkg.join("src/lib.rs"));
        if matches!(root, RepoRoot::Package(_)) {
            assert_eq!(rel, "src/lib.rs");
        }
        assert!(!rel.starts_with('/'), "never absolute: {rel}");
        assert!(
            !rel.contains(&*dir.path().to_string_lossy()),
            "no build path: {rel}"
        );

        let outside =
            RepoRoot::Package(pkg.clone()).relative(Path::new("/etc/elsewhere/secret.rs"));
        assert_eq!(outside, "secret.rs");
    }
}
