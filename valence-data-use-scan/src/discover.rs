//! Which packages a host's catalog scans.
//!
//! The host package is the deployment list. Everything it links (normal
//! dependencies) is scanned, plus every component it declares under
//! `[target.'cfg(any())'.dependencies]`: Cargo resolves and fetches those but
//! never builds them, so a host can name binaries that run elsewhere without
//! compiling them. Only packages whose dependency graph reaches a Valence crate
//! can declare uses, so the rest of the graph is skipped.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use cargo_metadata::{CargoOpt, DependencyKind, Metadata, MetadataCommand, Package};

use crate::DataUseScanError;

/// Platform string for inventory-only dependencies.
pub const INVENTORY_TARGET: &str = "cfg(any())";

/// Package name prefix shared by the Valence framework crates.
const VALENCE_PREFIX: &str = "uf-valence";

/// The package whose deployment the catalog describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPackage {
    /// The package running this build script, with the features it was compiled
    /// with (`CARGO_PKG_NAME` and `CARGO_FEATURE_*`). Use this from `build.rs`.
    FromBuildScript,
    /// A named workspace member and an explicit feature list, for tests and tooling.
    Named {
        /// Cargo package name of the host.
        package: String,
        /// Features of that package to resolve the graph with.
        features: Vec<String>,
    },
}

/// One package selected for scanning.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub path: PathBuf,
    /// From `[package].repository` (workspace inheritance resolved by cargo_metadata).
    pub repository: String,
    /// `true` for path packages (sources can change between builds).
    pub local: bool,
}

/// Result of resolving the host's deployment graph.
#[derive(Debug)]
pub struct Discovery {
    pub host: String,
    pub host_manifest: PathBuf,
    pub packages: Vec<PackageInfo>,
    pub inventory_components: usize,
}

/// Resolve the host package, its features, and the packages to scan.
pub fn discover(
    workspace_root: &std::path::Path,
    host: &HostPackage,
) -> Result<Discovery, DataUseScanError> {
    let manifest_path = workspace_root.join("Cargo.toml");
    let (host_name, requested) = match host {
        HostPackage::FromBuildScript => (env_var("CARGO_PKG_NAME")?, None),
        HostPackage::Named { package, features } => (package.clone(), Some(features.clone())),
    };

    let members = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()
        .map_err(metadata_error)?;
    let host_pkg = members
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == host_name)
        .ok_or_else(|| DataUseScanError::UnknownHostPackage {
            name: host_name.clone(),
        })?;
    let declared: Vec<String> = host_pkg.features.keys().cloned().collect();
    let features = match requested {
        None => features_from_env(&declared, std::env::vars()),
        Some(list) => validate_features(&host_name, &declared, list)?,
    };

    let mut command = MetadataCommand::new();
    command.manifest_path(&manifest_path);
    if !features.is_empty() {
        command.features(CargoOpt::SomeFeatures(
            features
                .iter()
                .map(|f| format!("{host_name}/{f}"))
                .collect(),
        ));
    }
    let metadata = command.exec().map_err(metadata_error)?;
    select_packages(&metadata, &host_name)
}

fn select_packages(metadata: &Metadata, host_name: &str) -> Result<Discovery, DataUseScanError> {
    let host_pkg = metadata
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == host_name)
        .ok_or_else(|| DataUseScanError::UnknownHostPackage {
            name: host_name.to_string(),
        })?;
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| DataUseScanError::Metadata {
            message: "cargo metadata returned no dependency resolution".into(),
        })?;

    let by_id: HashMap<&str, &Package> = metadata
        .packages
        .iter()
        .map(|p| (p.id.repr.as_str(), p))
        .collect();
    let mut edges: HashMap<&str, Vec<Edge<'_>>> = HashMap::new();
    for node in &resolve.nodes {
        let list = node
            .deps
            .iter()
            .map(|dep| Edge {
                to: dep.pkg.repr.as_str(),
                normal: dep
                    .dep_kinds
                    .iter()
                    .any(|k| k.kind == DependencyKind::Normal),
                inventory: dep.dep_kinds.iter().any(|k| {
                    k.target
                        .as_ref()
                        .is_some_and(|t| is_inventory_target(&t.to_string()))
                }),
            })
            .collect();
        edges.insert(node.id.repr.as_str(), list);
    }

    let host_id = host_pkg.id.repr.as_str();
    let inventory_resolved: BTreeSet<&str> = edges
        .get(host_id)
        .into_iter()
        .flatten()
        .filter(|e| e.inventory)
        .filter_map(|e| by_id.get(e.to).map(|p| p.name.as_str()))
        .collect();
    for dep in &host_pkg.dependencies {
        let declared_inventory = dep.kind == DependencyKind::Normal
            && !dep.optional
            && dep
                .target
                .as_ref()
                .is_some_and(|t| is_inventory_target(&t.to_string()));
        if declared_inventory && !inventory_resolved.contains(dep.name.as_str()) {
            return Err(DataUseScanError::InventoryDependencyWithoutLib {
                package: dep.name.clone(),
            });
        }
    }

    let reachable = closure(host_id, &edges);
    let is_valence = |id: &str| {
        by_id
            .get(id)
            .is_some_and(|p| p.name.as_str().starts_with(VALENCE_PREFIX))
    };
    let valence_users = reaching(&reachable, &edges, is_valence);

    let mut packages = Vec::new();
    for id in valence_users {
        let Some(pkg) = by_id.get(id) else { continue };
        if pkg.name.as_str().starts_with(VALENCE_PREFIX) {
            continue;
        }
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
            repository: pkg.repository.clone().unwrap_or_default(),
            local: pkg.source.is_none(),
        });
    }
    packages.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Discovery {
        host: host_name.to_string(),
        host_manifest: host_pkg.manifest_path.as_std_path().to_path_buf(),
        packages,
        inventory_components: inventory_resolved.len(),
    })
}

/// A resolved dependency edge.
#[derive(Debug, Clone, Copy)]
pub struct Edge<'a> {
    pub to: &'a str,
    /// At least one normal (non-dev, non-build) dependency kind.
    pub normal: bool,
    /// Declared under the inventory-only `cfg(any())` target.
    pub inventory: bool,
}

/// Packages reachable from `start` over normal edges (inventory edges are normal
/// edges with an always-false target, so they are followed too).
pub fn closure<'a>(start: &'a str, edges: &HashMap<&'a str, Vec<Edge<'a>>>) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        for edge in edges.get(id).into_iter().flatten() {
            if edge.normal && !seen.contains(edge.to) {
                stack.push(edge.to);
            }
        }
    }
    seen
}

/// Members of `nodes` that are, or transitively depend on, a package matching
/// `is_target` over normal edges.
pub fn reaching<'a>(
    nodes: &BTreeSet<&'a str>,
    edges: &HashMap<&'a str, Vec<Edge<'a>>>,
    is_target: impl Fn(&str) -> bool,
) -> BTreeSet<&'a str> {
    let mut hit: BTreeSet<&str> = nodes.iter().copied().filter(|id| is_target(id)).collect();
    loop {
        let before = hit.len();
        for id in nodes {
            if hit.contains(id) {
                continue;
            }
            let reaches = edges
                .get(id)
                .into_iter()
                .flatten()
                .any(|e| e.normal && hit.contains(e.to));
            if reaches {
                hit.insert(id);
            }
        }
        if hit.len() == before {
            return hit;
        }
    }
}

/// Map `CARGO_FEATURE_*` variables back to the host's declared feature names.
///
/// Cargo upper-cases names and turns `-` into `_`. Variables that match no
/// declared feature (implicit optional-dependency features) are skipped.
pub fn features_from_env(
    declared: &[String],
    vars: impl IntoIterator<Item = (String, String)>,
) -> Vec<String> {
    let by_env: BTreeMap<String, &String> = declared
        .iter()
        .map(|name| (name.to_uppercase().replace('-', "_"), name))
        .collect();
    let mut out: BTreeSet<String> = BTreeSet::new();
    for (key, _) in vars {
        if let Some(suffix) = key.strip_prefix("CARGO_FEATURE_") {
            if let Some(name) = by_env.get(suffix) {
                out.insert((*name).clone());
            }
        }
    }
    out.into_iter().collect()
}

fn validate_features(
    package: &str,
    declared: &[String],
    requested: Vec<String>,
) -> Result<Vec<String>, DataUseScanError> {
    for feature in &requested {
        if !declared.iter().any(|d| d == feature) {
            return Err(DataUseScanError::UnknownFeature {
                package: package.to_string(),
                feature: feature.clone(),
            });
        }
    }
    Ok(requested)
}

fn is_inventory_target(target: &str) -> bool {
    target.split_whitespace().collect::<String>() == INVENTORY_TARGET
}

fn env_var(name: &str) -> Result<String, DataUseScanError> {
    std::env::var(name).map_err(|_| DataUseScanError::MissingBuildScriptEnv {
        var: name.to_string(),
    })
}

fn metadata_error(e: cargo_metadata::Error) -> DataUseScanError {
    DataUseScanError::Metadata {
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(to: &str, normal: bool) -> Edge<'_> {
        Edge {
            to,
            normal,
            inventory: false,
        }
    }

    #[test]
    fn closure_follows_normal_and_inventory_edges_not_dev_or_build() {
        let mut edges: HashMap<&str, Vec<Edge<'_>>> = HashMap::new();
        edges.insert(
            "server",
            vec![
                edge("product", true),
                Edge {
                    to: "worker",
                    normal: true,
                    inventory: true,
                },
                edge("test-helpers", false),
            ],
        );
        edges.insert("worker", vec![edge("worker-lib", true)]);
        edges.insert("test-helpers", vec![edge("fixtures", true)]);

        let got: Vec<&str> = closure("server", &edges).into_iter().collect();
        assert_eq!(got, vec!["product", "server", "worker", "worker-lib"]);
    }

    #[test]
    fn reaching_marks_transitive_valence_users_only() {
        let mut edges: HashMap<&str, Vec<Edge<'_>>> = HashMap::new();
        edges.insert("server", vec![edge("facade", true), edge("serde", true)]);
        edges.insert("facade", vec![edge("uf-valence", true)]);
        edges.insert("serde", vec![]);
        let nodes: BTreeSet<&str> = ["server", "facade", "uf-valence", "serde"].into();

        let got: Vec<&str> = reaching(&nodes, &edges, |id| id == "uf-valence")
            .into_iter()
            .collect();
        assert_eq!(got, vec!["facade", "server", "uf-valence"]);
    }

    #[test]
    fn cargo_feature_env_maps_to_declared_features() {
        let declared = vec![
            "default".to_string(),
            "server-embedded".to_string(),
            "ssr".to_string(),
        ];
        let vars = vec![
            ("CARGO_FEATURE_SERVER_EMBEDDED".to_string(), "1".to_string()),
            ("CARGO_FEATURE_DEFAULT".to_string(), "1".to_string()),
            ("CARGO_FEATURE_IMPLICIT_DEP".to_string(), "1".to_string()),
            ("CARGO_PKG_NAME".to_string(), "server".to_string()),
        ];
        assert_eq!(
            features_from_env(&declared, vars),
            vec!["default".to_string(), "server-embedded".to_string()]
        );
    }

    #[test]
    fn named_features_must_be_declared() {
        let declared = vec!["ssr".to_string()];
        assert_eq!(
            validate_features("server", &declared, vec!["ssr".into()]).unwrap(),
            vec!["ssr".to_string()]
        );
        let err = validate_features("server", &declared, vec!["hydrate".into()]).unwrap_err();
        assert!(matches!(
            err,
            DataUseScanError::UnknownFeature { ref package, ref feature }
                if package == "server" && feature == "hydrate"
        ));
    }

    #[test]
    fn inventory_target_tolerates_whitespace() {
        assert!(is_inventory_target("cfg(any())"));
        assert!(is_inventory_target("cfg( any () )"));
        assert!(!is_inventory_target("cfg(any(unix))"));
        assert!(!is_inventory_target("cfg(windows)"));
    }
}
