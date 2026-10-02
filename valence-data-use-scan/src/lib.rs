//! Build-time catalog of every Valence data use a deployment declares.
//!
//! Valence read and write APIs take a `use_!(…)` purpose explaining, in plain
//! language, why the code touches that data. This crate finds those call sites
//! across everything a deployment runs and generates a `data_use_catalog()`
//! function that the host installs at boot, so end users can see how their data
//! is used.
//!
//! ## Features
//!
//! - **Deployment scan** — Collects every purpose-required call in the host
//!   package and everything it links, including product crates pulled from git,
//!   with the features the host was compiled with. Call [`generate`] once from the
//!   host's `build.rs`. [Get started](#getting-started)
//! - **Deployment inventory** — Lets the host name components that run as their
//!   own binaries (Chronon, Boson, or Photon runtimes, workers in other
//!   repositories) so their uses appear too, without compiling them into the
//!   host. [Declare deployment components](#declare-deployment-components)
//! - **Purpose extraction** — Reads the `use_!(…)` markdown next to each
//!   purpose-required call so the catalog shows end-user trust copy rather than
//!   method names. [Get started](#getting-started)
//! - **Target classification** — Sorts each use into Schema, Trait, or Unscoped so
//!   the ops UI can show it on the right schema card, trait card, or Unscoped page.
//!   [Get started](#getting-started)
//! - **Purpose quality lint** — [`lint_purpose()`] checks trust-copy banlists and
//!   tier depth so migration templates cannot re-land. [Get started](#lint-a-purpose-string)
//! - **Test exclusion** — When [`Config::exclude_tests_from_snapshot`] is set, omits
//!   test modules inside `src/` (`tests.rs`, `*_test.rs`, `tests/` directories) from
//!   the generated catalog so fixture twins stay out of operator views.
//!   [Get started](#exclude-tests-from-the-snapshot)
//! - **Connection hops** — Classifies forward `get_*` / `relate_to_*` hops
//!   and optionally bakes peer schema via [`Config::connection_edges`] for Referenced
//!   Reads / Updates in valence-uf-app. [Get started](#attribute-connection-hops)
//!
//! ## Getting started
//!
//! The catalog answers "how does this deployment use my data?" for end users, so
//! it has to cover the whole deployment rather than one crate. The host server
//! package owns that list: [`generate`] runs from its `build.rs` each time Cargo
//! builds the host, and the host installs the result once at boot. The pass scans the `src/` tree of the
//! host and every crate it links. Top-level `tests/`, `examples/`, and `benches/`
//! never ship, so they are skipped. Components that run as separate binaries are
//! added in [Declare deployment components](#declare-deployment-components).
//!
//! ### Prerequisites
//!
//! - This crate on the host's `[build-dependencies]`.
//! - `uf-valence` available to the host as `valence` (the generated function
//!   returns `valence::data_use::DataUseCatalog`).
//! - Product crates that call purpose-required APIs with `use_!` purposes.
//!
//! ### Wire generate from build.rs
//!
//! ```rust,no_run
//! use std::path::PathBuf;
//! use valence_data_use_scan::{generate, Config, HostPackage};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
//!     let workspace_root = manifest_dir.parent().ok_or("host has no parent")?.to_path_buf();
//!     generate(&Config {
//!         workspace_root,
//!         out_dir: PathBuf::from(std::env::var("OUT_DIR")?),
//!         host: HostPackage::FromBuildScript,
//!         exclude_tests_from_snapshot: true,
//!         connection_edges: vec![],
//!     })?;
//!     Ok(())
//! }
//! ```
//!
//! [`HostPackage::FromBuildScript`] reads the building package's name and its
//! enabled features from Cargo, so optional product crates are catalogued
//! exactly when the host compiles them. On success the pass writes
//! `data_uses.rs` under `out_dir`. Include it once in the host and install the
//! catalog at boot:
//!
//! ```rust,ignore
//! mod generated {
//!     include!(concat!(env!("OUT_DIR"), "/data_uses.rs"));
//! }
//!
//! let catalog = generated::data_use_catalog().install()?;
//! println!("data-use catalog rows: {}", catalog.len());
//! ```
//!
//! Observable outcome: `data_use_catalog()` returns one row per declared use,
//! with `file` relative to each crate's repository root, and readers find it
//! through `DataUseCatalog::global()`. Metadata or parse failures return
//! [`DataUseScanError`]; let `build.rs` fail rather than ship an empty catalog.
//!
//! ### Declare deployment components
//!
//! A deployment usually runs more than the web server: Chronon, Boson, or Photon
//! runtimes, workers from other repositories, any binary the owner ships. Their
//! uses belong in the same catalog, but only the host owner knows which
//! binaries the deployment runs and where they come from, so the host declares
//! them. List each one under `[target.'cfg(any())'.dependencies]` in the host
//! package's `Cargo.toml`. `cfg(any())` is never true, so when the host builds,
//! Cargo resolves, locks, and downloads these crates but never compiles or links
//! them, and the scan reads their sources. Do this when you add a deployed
//! binary, in the same change that adds it to your deploy.
//!
//! Prerequisites: the host's `build.rs` uses [`HostPackage::FromBuildScript`]
//! as above, and each component exposes a library target. A binary-only crate
//! needs a one-line `src/lib.rs` (a doc comment is enough), because Cargo drops
//! lib-less dependencies.
//!
//! ```toml
//! # server/Cargo.toml
//! # Data-use inventory for binaries this deployment runs outside the web server.
//! # cfg(any()) is never true: Cargo locks and fetches these but never builds them.
//! [target.'cfg(any())'.dependencies]
//! chronon-runtime = { path = "../chronon-runtime" }
//! ocr-chronon-worker = { git = "https://github.com/acme/ocr-worker", branch = "main", features = ["s3"] }
//! ```
//!
//! Features on an entry choose which optional crates of that component count,
//! matching how the owner builds it. A host test then pins the deployment so a
//! forgotten component fails CI:
//!
//! ```rust,ignore
//! #[test]
//! fn catalog_includes_every_deployed_component() {
//!     // `host: HostPackage::FromBuildScript` in build.rs; generated module included above.
//!     let catalog = generated::data_use_catalog();
//!     let crates = catalog.crate_names();
//!     assert!(crates.contains("chronon-runtime"));
//!     assert!(crates.contains("ocr-chronon-worker"));
//! }
//! ```
//!
//! Observable outcome: uses declared in those components appear in the catalog
//! with their own crate names and repository-relative paths, and nothing from
//! them is compiled into the host. A non-optional inventory entry without a
//! library target fails the build with
//! [`DataUseScanError::InventoryDependencyWithoutLib`]. Components the owner
//! never declares are invisible, so keep the list next to your deploy config
//! and add a host test like the one above. Next: install the catalog at boot
//! ([Valence guide](https://docs.rs/uf-valence/latest/valence/#install-the-data-use-catalog-at-boot)).
//!
//! ### Lint a purpose string
//!
//! ```rust
//! use valence_data_use_scan::{lint_purpose, purpose_passes, PurposeTier};
//!
//! let purpose = "Before we begin **enrollment** on setting up your **authenticator**, we first verify the user account **exists** by **loading it with the provided id**. **No other information** is required, so we discard it immediately.";
//! assert!(purpose_passes(purpose, PurposeTier::S3));
//! assert!(lint_purpose("get User in src/x.rs; Valence persistence for this feature path; typed store; visible to session actor / service path.", PurposeTier::S3).len() > 0);
//! ```
//!
//! Observable outcome: empty gap list means Pass; gap codes name the failure.
//!
//! ### Exclude tests from the snapshot
//!
//! Set [`Config::exclude_tests_from_snapshot`] to `true` when unit-test modules under
//! `src/` mirror production purpose-required calls with twin purposes that must not
//! appear in the UI.
//! Leave it `false` only when you intentionally want test fixtures in the catalog.
//!
//! ```rust,no_run
//! use std::path::PathBuf;
//! use valence_data_use_scan::{generate, Config, HostPackage};
//!
//! fn main() {
//!     generate(&Config {
//!         workspace_root: PathBuf::from("."),
//!         out_dir: PathBuf::from("out"),
//!         host: HostPackage::FromBuildScript,
//!         exclude_tests_from_snapshot: true,
//!         connection_edges: vec![],
//!     })
//!     .expect("data-use scan");
//!     println!("excluded tests from the data-use catalog");
//! }
//! ```
//!
//! ### Attribute connection hops
//!
//! Connection hop attribution lets the Valence ops UI show inbound loads and
//! edge mutates on the **peer** schema's Data uses page (Referenced Reads /
//! Updates). The scan attaches an optional hop from method names
//! (`get_{field}`, `relate_to_*` / `unrelate_from_*`). Pass
//! [`Config::connection_edges`] to bake `referenced_schema` at generate time
//! (e2e / unit determinism). Product hosts may leave edges empty and resolve
//! peers at SSR from `SchemaRegistry`.
//!
//! ```rust,no_run
//! use std::path::PathBuf;
//! use valence_data_use_scan::{generate, Config, ConnectionEdge, HostPackage};
//!
//! fn main() {
//!     generate(&Config {
//!         workspace_root: PathBuf::from("."),
//!         out_dir: PathBuf::from("out"),
//!         host: HostPackage::FromBuildScript,
//!         exclude_tests_from_snapshot: true,
//!         connection_edges: vec![ConnectionEdge {
//!             from_table: "todo".into(),
//!             from_field: "owner".into(),
//!             to_table: "user".into(),
//!         }],
//!     })
//!     .expect("data-use scan");
//!     println!("baked peer attribution for todo.owner → user");
//! }
//! ```
//!
//! Next: browse schema / trait / Unscoped Data uses in `valence-app` after mounting
//! `/valence` routes. Schema pages also show **Referenced Reads** / **Referenced
//! Updates** when a peer was loaded or edge-updated via a connection.
//!
//! ## Examples
//!
//! `tests/deployment_inventory.rs` builds a host workspace with an out-of-tree
//! product, an inventory-only worker, and an undeclared binary, and checks what
//! the catalog contains (`cargo test -p uf-valence-data-use-scan`).

#![deny(clippy::missing_errors_doc)]

mod classify;
mod discover;
mod emit;
mod exclude;
mod inventory;
pub mod lint_purpose;
mod repo_path;
mod scan;

pub use discover::HostPackage;
pub use inventory::{inventory_csv_header, inventory_csv_row, lint_scan_hits, InventoryRow};
pub use lint_purpose::{lint_purpose, purpose_passes, GapCode, PurposeTier};

use std::path::{Path, PathBuf};
use std::time::Instant;

use classify::{
    classify_connection_hop, classify_method, classify_target, hop_field_matches_connection,
};
use discover::{discover, Discovery, PackageInfo};
use exclude::should_exclude_path;
use repo_path::RepoRoot;
use scan::{scan_file, FoundUse};

/// Failure while scanning purpose-required call sites or writing the snapshot.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DataUseScanError {
    /// `cargo_metadata` could not load the workspace manifest.
    #[error("cargo metadata: {message}")]
    Metadata {
        /// Human-readable failure detail (no secrets).
        message: String,
    },
    /// The host package is not a member of the workspace at `workspace_root`.
    #[error("host package `{name}` is not a member of this workspace")]
    UnknownHostPackage {
        /// Package name that was looked up.
        name: String,
    },
    /// [`HostPackage::Named`] listed a feature the host package does not declare.
    #[error("package `{package}` has no feature `{feature}`")]
    UnknownFeature {
        /// Host package name.
        package: String,
        /// Feature that is not declared.
        feature: String,
    },
    /// An inventory-only (`cfg(any())`) dependency has no library target, so
    /// Cargo dropped it from the graph and its uses cannot be catalogued.
    #[error(
        "inventory dependency `{package}` has no library target; add a doc-only src/lib.rs so Cargo keeps it"
    )]
    InventoryDependencyWithoutLib {
        /// Package name of the lib-less dependency.
        package: String,
    },
    /// [`HostPackage::FromBuildScript`] ran outside a Cargo build script.
    #[error("{var} is not set; HostPackage::FromBuildScript only works inside build.rs")]
    MissingBuildScriptEnv {
        /// Environment variable Cargo normally provides.
        var: String,
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
    /// Root of the workspace that contains the host package (holds `Cargo.toml`).
    pub workspace_root: PathBuf,
    /// Output directory where `data_uses.rs` will be written.
    pub out_dir: PathBuf,
    /// Host package whose linked crates and `cfg(any())` inventory deps are scanned.
    pub host: HostPackage,
    /// When true, omit test modules under `src/` (`tests.rs`, `*_test.rs`, `tests/`
    /// directories) from the catalog.
    pub exclude_tests_from_snapshot: bool,
    /// Optional connection edges used to bake [`ScanHit::referenced_schema`] at
    /// generate time. Empty in product hosts that resolve peers at SSR.
    pub connection_edges: Vec<ConnectionEdge>,
}

/// One schema connection edge for peer bake (`from_table.from_field` → `to_table`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionEdge {
    /// Initiating schema table name (snake_case).
    pub from_table: String,
    /// Connection `from_field` / codegen connection name.
    pub from_field: String,
    /// Peer schema table name (snake_case).
    pub to_table: String,
}

/// Kind of connection hop attributed for Referenced Reads / Updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionHopKind {
    /// Forward `get_{field}` / `get_{field}_record_ids`.
    ForwardGet,
    /// `relate_to_*` / `unrelate_from_*`.
    Relate,
}

/// Parsed connection hop from a purpose-required method name (no rustc types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionHop {
    /// Connection field token extracted from the method name.
    pub field: String,
    /// Forward load vs edge mutate.
    pub kind: ConnectionHopKind,
}

/// One catalog row written into the generated snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHit {
    /// Markdown purpose text from `use_!`.
    pub purpose: String,
    /// Path relative to the root of the repository that owns the package.
    pub file: String,
    /// 1-based line of the purpose-required call (best effort from the `use_!` / call span).
    pub line: u32,
    /// Cargo package name that owns the file.
    pub crate_name: String,
    /// Package `repository` from Cargo.toml (View source for Unscoped rows).
    pub repository: String,
    /// Schema / Trait / Unscoped classification.
    pub target: TargetKind,
    /// CRUD-shaped op bucket for UI tabs.
    pub op: OpKind,
    /// Method name (`get`, `query`, …).
    pub method: String,
    /// Connection field when this call is a forward hop or edge mutate.
    pub connection_field: Option<String>,
    /// Hop kind when [`Self::connection_field`] is set.
    pub connection_kind: Option<ConnectionHopKind>,
    /// Peer schema baked from [`Config::connection_edges`], when resolvable.
    pub referenced_schema: Option<String>,
}

/// Target classification mirrored in the generated snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    /// Typed Model / schema access (`User::get` → `user`).
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

/// Scan the host's deployment and write `OUT_DIR/data_uses.rs`.
///
/// Call once from the host package's `build.rs`. The scan covers the host,
/// every crate it links, and every `[target.'cfg(any())'.dependencies]`
/// inventory component, keeping only packages whose dependency graph reaches
/// Valence, and reads each package's `src/` tree. Prefer
/// [`Config::exclude_tests_from_snapshot`] `true` for operator-facing catalogs.
///
/// # Errors
///
/// Returns [`DataUseScanError::UnknownHostPackage`] or
/// [`DataUseScanError::UnknownFeature`] for a misnamed host or feature,
/// [`DataUseScanError::InventoryDependencyWithoutLib`] for a lib-less inventory
/// entry, [`DataUseScanError::MissingBuildScriptEnv`] when
/// [`HostPackage::FromBuildScript`] runs outside `build.rs`, and the metadata,
/// I/O, or parse variants when Cargo metadata cannot load, a source file cannot
/// be read or parsed, or the snapshot cannot be written.
pub fn generate(config: &Config) -> Result<(), DataUseScanError> {
    let started = Instant::now();
    let discovery = discover(&config.workspace_root, &config.host)?;
    let hits = hits_for(config, &discovery.packages)?;

    emit::write_snapshot(&config.out_dir, &hits)?;

    let duration_ms = started.elapsed().as_millis();
    tracing::info!(
        target: "valence.data_use.scan",
        host_package = %discovery.host,
        inventory_components = discovery.inventory_components,
        crate_count = discovery.packages.len(),
        use_count = hits.len(),
        duration_ms = duration_ms as u64,
        "data-use scan complete"
    );

    // Build-script directives must go to stdout for cargo.
    #[allow(clippy::print_stdout)]
    for path in rerun_paths(&config.workspace_root, &discovery) {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    Ok(())
}

/// Collect every purpose-required + `use_!` hit for the host's deployment
/// without writing the snapshot.
///
/// # Errors
///
/// Same as [`generate`] except snapshot write failures.
pub fn collect_hits(config: &Config) -> Result<Vec<ScanHit>, DataUseScanError> {
    let discovery = discover(&config.workspace_root, &config.host)?;
    hits_for(config, &discovery.packages)
}

fn hits_for(config: &Config, packages: &[PackageInfo]) -> Result<Vec<ScanHit>, DataUseScanError> {
    let mut hits: Vec<ScanHit> = Vec::new();
    for package in packages {
        hits.extend(scan_package(config, package)?);
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
    Ok(hits)
}

/// Files whose change should rerun the scan: manifests, the lockfile, and the
/// directories of path packages (registry and git sources are pinned by the lock).
fn rerun_paths(workspace_root: &Path, discovery: &Discovery) -> Vec<PathBuf> {
    let mut paths = vec![
        workspace_root.join("Cargo.toml"),
        workspace_root.join("Cargo.lock"),
        discovery.host_manifest.clone(),
    ];
    for package in discovery.packages.iter().filter(|p| p.local) {
        paths.push(package.path.join("Cargo.toml"));
        paths.push(package.path.join("src"));
    }
    paths.sort();
    paths.dedup();
    paths
}

fn scan_package(config: &Config, package: &PackageInfo) -> Result<Vec<ScanHit>, DataUseScanError> {
    let repo_root = RepoRoot::detect(&package.path);
    let mut hits = Vec::new();
    let src = package.path.join("src");
    if !src.is_dir() {
        return Ok(hits);
    }
    let walker = ignore::WalkBuilder::new(&src)
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
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let in_package = path.strip_prefix(&package.path).unwrap_or(path);
        if in_package.components().any(|c| c.as_os_str() == "target") {
            continue;
        }
        if config.exclude_tests_from_snapshot && should_exclude_path(in_package) {
            continue;
        }

        let rel = repo_root.relative(path);
        for item in scan_file(path, &package.name, &rel)? {
            hits.push(hit_from_found(
                item,
                &package.repository,
                &config.connection_edges,
            ));
        }
    }
    Ok(hits)
}

fn hit_from_found(found: FoundUse, repository: &str, edges: &[ConnectionEdge]) -> ScanHit {
    let op = classify_method(&found.method);
    let target = classify_target(&found.receiver, &found.method);
    let hop = classify_connection_hop(&found.method);
    let referenced_schema = match (&target, &hop) {
        (TargetKind::Schema(from_table), Some(hop)) => {
            bake_referenced_schema(from_table, hop, edges)
        }
        _ => None,
    };
    ScanHit {
        purpose: found.purpose,
        file: found.file,
        line: found.line,
        crate_name: found.crate_name,
        repository: repository.to_string(),
        target,
        op,
        method: found.method,
        connection_field: hop.as_ref().map(|h| h.field.clone()),
        connection_kind: hop.map(|h| h.kind),
        referenced_schema,
    }
}

fn bake_referenced_schema(
    from_table: &str,
    hop: &ConnectionHop,
    edges: &[ConnectionEdge],
) -> Option<String> {
    edges
        .iter()
        .find(|e| {
            e.from_table == from_table && hop_field_matches_connection(&hop.field, &e.from_field)
        })
        .map(|e| e.to_table.clone())
}
