//! Build-time catalog of Valence `*_used` + `use_!` call sites for the
//! valence-uf-app Data uses UI.
//!
//! Walks workspace member `.rs` sources with `syn`, pairs each `*_used` method call
//! with a nearby `use_!(…)` purpose, and writes `data_uses.rs` under `OUT_DIR` for
//! host `build.rs` to `include!`.
//!
//! ## Features
//!
//! - **Workspace scan** — Discovers `*_used` calls across Cargo workspace members
//!   (via `cargo_metadata` plus a directory walk) so host SSR can ship a static
//!   catalog. Call [`generate`] once from `build.rs` at compile time.
//!   [Get started](#getting-started)
//! - **Purpose extraction** — Reads `use_!("…")` / `use_!(r#"…"#)` arguments next to
//!   each `*_used` call so the UI can show end-user trust copy.
//!   [Get started](#getting-started)
//! - **Target classification** — Maps receivers to Schema / Trait / Unscoped for the
//!   valence-uf-app Data uses surfaces (schema card, trait card, Unscoped page).
//!   [Get started](#getting-started)
//! - **Test exclusion** — When [`Config::exclude_tests_from_snapshot`] is set, omits
//!   `tests/` paths and `*_test.rs` files from the generated UI snapshot so fixture
//!   twins stay out of operator views. [Get started](#exclude-tests-from-the-snapshot)
//!
//! ## Getting started
//!
//! `uf-valence-data-use-scan` turns declared `*_used` / `use_!` call sites into a
//! static `DATA_USES` slice for the Valence ops UI. Call [`generate`] from a host
//! `build.rs` when the host crate builds (once per Cargo compile) after adding this
//! crate as a `build-dependency`, with [`Config::workspace_root`] pointed at the
//! Cargo workspace that owns the product crates you want catalogued.
//!
//! ### Prerequisites
//!
//! - This crate on `[build-dependencies]` of the host (for example `valence-app`).
//! - Workspace members that already call `*_used` with `use_!` purposes.
//! - `OUT_DIR` available to the build script (Cargo provides it).
//!
//! ### Wire generate from build.rs
//!
//! ```rust,no_run
//! use std::path::PathBuf;
//! use valence_data_use_scan::{generate, Config, DataUseScanError};
//!
//! fn main() -> Result<(), DataUseScanError> {
//!     let workspace_root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
//!         .parent()
//!         .expect("host crate parent is workspace root")
//!         .to_path_buf();
//!     generate(&Config {
//!         workspace_root,
//!         out_dir: PathBuf::from(std::env::var("OUT_DIR").unwrap()),
//!         exclude_tests_from_snapshot: true,
//!     })?;
//!     Ok(())
//! }
//! ```
//!
//! On success the pass writes `data_uses.rs` under `out_dir` with a `DATA_USES`
//! static slice. Include it from SSR code:
//!
//! ```rust,ignore
//! include!(concat!(env!("OUT_DIR"), "/data_uses.rs"));
//! println!("data-use catalog rows: {}", DATA_USES.len());
//! ```
//!
//! Observable outcome: `data_uses.rs` exists under `OUT_DIR`, and the slice lists
//! Schema / Trait / Unscoped rows for scanned purposes. Metadata or parse failures
//! return [`DataUseScanError`] (host `build.rs` should fail loud, not ship an empty
//! catalog as success).
//!
//! ### Exclude tests from the snapshot
//!
//! Set [`Config::exclude_tests_from_snapshot`] to `true` when product tests mirror
//! production `*_used` calls with twin purposes that must not appear in the UI.
//! Leave it `false` only when you intentionally want test fixtures in the catalog.
//!
//! ```rust,no_run
//! use std::path::PathBuf;
//! use valence_data_use_scan::{generate, Config};
//!
//! fn main() {
//!     generate(&Config {
//!         workspace_root: PathBuf::from("."),
//!         out_dir: PathBuf::from("out"),
//!         exclude_tests_from_snapshot: true,
//!     })
//!     .expect("data-use scan");
//!     println!("excluded tests from DATA_USES snapshot");
//! }
//! ```
//!
//! Next: browse schema / trait / Unscoped Data uses in `valence-app` after mounting
//! `/valence` routes.
//!
//! ## Examples
//!
//! Unit coverage of the fixture workspace lives in this crate's tests
//! (`generate_excludes_tests_when_configured`). Host wiring: `valence-app/build.rs`.

#![deny(clippy::missing_errors_doc)]

mod classify;
mod emit;
mod exclude;
mod scan;

use std::path::PathBuf;
use std::time::Instant;

use classify::{classify_method, classify_target};
use exclude::should_exclude_path;
use scan::{scan_file, FoundUse};

/// Failure while scanning `*_used` call sites or writing the snapshot.
#[derive(Debug, thiserror::Error)]
pub enum DataUseScanError {
    /// `cargo_metadata` could not load the workspace manifest.
    #[error("cargo metadata: {message}")]
    Metadata {
        /// Human-readable failure detail (no secrets).
        message: String,
    },
    /// A package manifest path had no parent directory.
    #[error("package path for {manifest}: {message}")]
    PackagePath {
        /// Manifest path that lacked a parent.
        manifest: String,
        /// Human-readable failure detail.
        message: String,
    },
    /// Reading or writing a file on disk failed.
    #[error("I/O on {path}: {message}")]
    Io {
        /// Path that could not be read or written.
        path: PathBuf,
        /// Underlying I/O message.
        message: String,
    },
    /// A source file could not be parsed by `syn`.
    #[error("parse {path}: {message}")]
    Parse {
        /// Path that failed to parse.
        path: PathBuf,
        /// Syn / lexer message.
        message: String,
    },
}

/// Configuration for the data-use catalog scan.
pub struct Config {
    /// Root workspace directory (contains `Cargo.toml`).
    pub workspace_root: PathBuf,
    /// Output directory where `data_uses.rs` will be written.
    pub out_dir: PathBuf,
    /// When true, omit `tests/` paths and `*_test.rs` files from the UI snapshot.
    pub exclude_tests_from_snapshot: bool,
}

/// One catalog row written into the generated snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHit {
    /// Markdown purpose text from `use_!`.
    pub purpose: String,
    /// Repo-root-relative source path.
    pub file: String,
    /// 1-based line of the `*_used` call (best effort from the `use_!` / call span).
    pub line: u32,
    /// Cargo package name that owns the file.
    pub crate_name: String,
    /// Schema / Trait / Unscoped classification.
    pub target: TargetKind,
    /// CRUD-shaped op bucket for UI tabs.
    pub op: OpKind,
    /// Method name (`get_used`, `query_used`, …).
    pub method: String,
}

/// Target classification mirrored in the generated snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    /// Typed Model / schema access (`User::get_used` → `user`).
    Schema(String),
    /// Trait QueryAll / trait helpers (`NamedQueryAll` → `Named`).
    Trait(String),
    /// QueryCore / raw unscoped entry points.
    Unscoped,
}

/// Op bucket mirrored in the generated snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// Reads / queries / get_mutable.
    Read,
    /// Creates.
    Create,
    /// Updates / merges / upserts.
    Update,
    /// Deletes (including delete_now).
    Delete,
}

impl OpKind {
    /// Stable snake_case label for generated code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// Scan workspace members and write `OUT_DIR/data_uses.rs`.
///
/// Call once from host `build.rs` after backends and product crates are on the
/// workspace member list. Prefer [`Config::exclude_tests_from_snapshot`] `true` for
/// operator-facing catalogs.
///
/// # Errors
///
/// Returns [`DataUseScanError`] when Cargo metadata cannot be loaded, a package
/// path is invalid, a source file cannot be read/parsed, or the snapshot cannot
/// be written.
pub fn generate(config: &Config) -> Result<(), DataUseScanError> {
    let started = Instant::now();
    let packages = discover_packages(config)?;
    let mut hits: Vec<ScanHit> = Vec::new();

    for package in &packages {
        let package_hits = scan_package(config, package)?;
        hits.extend(package_hits);
    }

    hits.sort_by(|a, b| {
        (
            a.crate_name.as_str(),
            a.file.as_str(),
            a.line,
            a.method.as_str(),
            a.purpose.as_str(),
        )
            .cmp(&(
                b.crate_name.as_str(),
                b.file.as_str(),
                b.line,
                b.method.as_str(),
                b.purpose.as_str(),
            ))
    });
    hits.dedup_by(|a, b| {
        a.crate_name == b.crate_name
            && a.file == b.file
            && a.line == b.line
            && a.method == b.method
            && a.purpose == b.purpose
    });

    emit::write_snapshot(&config.out_dir, &hits)?;

    let duration_ms = started.elapsed().as_millis();
    tracing::info!(
        target: "valence.data_use.scan",
        crate_count = packages.len(),
        use_count = hits.len(),
        duration_ms = duration_ms as u64,
        "data-use scan complete"
    );

    // Build-script directives must go to stdout for cargo.
    #[allow(clippy::print_stdout)]
    {
        println!(
            "cargo:rerun-if-changed={}",
            config.workspace_root.join("Cargo.toml").display()
        );
        for package in &packages {
            println!(
                "cargo:rerun-if-changed={}",
                package.path.join("Cargo.toml").display()
            );
        }
    }

    Ok(())
}

/// Package root discovered from Cargo metadata.
struct PackageInfo {
    name: String,
    path: PathBuf,
}

fn discover_packages(config: &Config) -> Result<Vec<PackageInfo>, DataUseScanError> {
    use cargo_metadata::MetadataCommand;

    let manifest_path = config.workspace_root.join("Cargo.toml");
    let metadata = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()
        .map_err(|e| DataUseScanError::Metadata {
            message: e.to_string(),
        })?;

    let mut packages = Vec::new();
    for member_id in &metadata.workspace_members {
        let Some(pkg) = metadata.packages.iter().find(|p| &p.id == member_id) else {
            continue;
        };
        let path = pkg
            .manifest_path
            .parent()
            .map(|dir| dir.as_std_path().to_path_buf())
            .ok_or_else(|| DataUseScanError::PackagePath {
                manifest: pkg.manifest_path.to_string(),
                message: "manifest path has no parent directory".to_string(),
            })?;
        packages.push(PackageInfo {
            name: pkg.name.to_string(),
            path,
        });
    }
    Ok(packages)
}

fn scan_package(config: &Config, package: &PackageInfo) -> Result<Vec<ScanHit>, DataUseScanError> {
    let mut hits = Vec::new();
    let walker = ignore::WalkBuilder::new(&package.path)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .build();

    for entry in walker {
        let entry = entry.map_err(|e| DataUseScanError::Io {
            path: package.path.clone(),
            message: e.to_string(),
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // Skip build artifacts and dependency sources if they appear under the package.
        let path_str = path.to_string_lossy();
        if path_str.contains("/target/") || path_str.contains("\\target\\") {
            continue;
        }
        if config.exclude_tests_from_snapshot && should_exclude_path(path) {
            continue;
        }

        let rel = relative_to_workspace(config, path);
        let found = scan_file(path, &package.name, &rel)?;
        for item in found {
            hits.push(hit_from_found(item));
        }
    }
    Ok(hits)
}

fn relative_to_workspace(config: &Config, path: &std::path::Path) -> String {
    path.strip_prefix(&config.workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn hit_from_found(found: FoundUse) -> ScanHit {
    let op = classify_method(&found.method);
    let target = classify_target(&found.receiver, &found.method);
    ScanHit {
        purpose: found.purpose,
        file: found.file,
        line: found.line,
        crate_name: found.crate_name,
        target,
        op,
        method: found.method,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_fixture_workspace(root: &std::path::Path) {
        fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
resolver = "2"
members = ["prod_crate"]
"#,
        )
        .unwrap();
        let prod = root.join("prod_crate");
        fs::create_dir_all(prod.join("src")).unwrap();
        fs::create_dir_all(prod.join("tests")).unwrap();
        fs::write(
            prod.join("Cargo.toml"),
            r#"[package]
name = "prod_crate"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        fs::write(
            prod.join("src/lib.rs"),
            include_str!("../tests/fixtures/prod_lib.rs"),
        )
        .unwrap();
        fs::write(
            prod.join("tests/integration.rs"),
            include_str!("../tests/fixtures/test_twin.rs"),
        )
        .unwrap();
    }

    #[test]
    fn generate_excludes_tests_when_configured() {
        let dir = tempdir().unwrap();
        write_fixture_workspace(dir.path());
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();

        generate(&Config {
            workspace_root: dir.path().to_path_buf(),
            out_dir: out.clone(),
            exclude_tests_from_snapshot: true,
        })
        .unwrap();

        let generated = fs::read_to_string(out.join("data_uses.rs")).unwrap();
        assert!(generated.contains("Load the user for the session cookie."));
        assert!(
            !generated.contains("TEST_ONLY_PURPOSE"),
            "test twin purpose must be excluded from UI snapshot"
        );
        assert!(generated.contains("DataUseTarget::Schema"));
        assert!(generated.contains("DataUseTarget::Trait"));
        assert!(generated.contains("DataUseTarget::Unscoped"));
    }

    #[test]
    fn generate_includes_tests_when_not_excluded() {
        let dir = tempdir().unwrap();
        write_fixture_workspace(dir.path());
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();

        generate(&Config {
            workspace_root: dir.path().to_path_buf(),
            out_dir: out.clone(),
            exclude_tests_from_snapshot: false,
        })
        .unwrap();

        let generated = fs::read_to_string(out.join("data_uses.rs")).unwrap();
        assert!(generated.contains("TEST_ONLY_PURPOSE"));
    }
}
