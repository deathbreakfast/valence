//! Inventory ledger: every scanned `use_!` with purpose lint score.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::exclude::should_exclude_path;
use crate::lint_purpose::{lint_purpose, GapCode, PurposeTier};
use crate::scan::scan_file;
use crate::{DataUseScanError, ScanHit};

/// One inventory row for CSV / acceptance gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryRow {
    /// Repo-relative file.
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// Crate name.
    pub crate_name: String,
    /// Method (`get`, …).
    pub method: String,
    /// Purpose markdown.
    pub purpose: String,
    /// Inferred tier.
    pub tier: PurposeTier,
    /// Pass when `gaps` empty.
    pub pass: bool,
    /// Gap codes (empty = Pass).
    pub gaps: Vec<GapCode>,
}

/// Lint each hit with [`PurposeTier::from_path`].
#[must_use]
pub fn lint_scan_hits(hits: &[ScanHit]) -> Vec<InventoryRow> {
    hits.iter()
        .map(|h| {
            let tier = PurposeTier::from_path(&h.file);
            let gaps = lint_purpose(&h.purpose, tier);
            InventoryRow {
                file: h.file.clone(),
                line: h.line,
                crate_name: h.crate_name.clone(),
                method: h.method.clone(),
                purpose: h.purpose.clone(),
                tier,
                pass: gaps.is_empty(),
                gaps,
            }
        })
        .collect()
}

/// Result of linting every `use_!` site under a directory tree.
#[derive(Debug)]
pub struct TreeLint {
    /// One row per `use_!` site, in path then line order.
    pub rows: Vec<InventoryRow>,
    /// Files that could not be read or parsed. Never silently dropped.
    pub skipped: Vec<(PathBuf, DataUseScanError)>,
}

/// Lint every `use_!` site in the `*.rs` files under `root`, with no cargo metadata.
///
/// For contributors rewriting purposes in a checkout. Hidden directories are
/// walked; git-ignored paths, `target/`, and test paths (see the data-use
/// snapshot's test exclusion) are skipped. `InventoryRow::file` is relative to `root` with `/`
/// separators, and tier inference runs on that path. The crate name is the
/// Lint every `use_!` site in the `*.rs` files under `root`, with no cargo metadata.
///
/// For contributors rewriting purposes in a checkout. Hidden directories are
/// walked; git-ignored paths, `target/`, and test paths (see the data-use
/// snapshot's test exclusion) are skipped. `InventoryRow::file` is relative to `root` with `/`
/// separators, and tier inference runs on that path. The crate name is the
/// `[package].name` of the nearest `Cargo.toml` at or above the file (up to
/// `root`), else `root`'s directory name. A file that cannot be read or parsed
/// lands in [`TreeLint::skipped`] and the rest are still linted.
///
/// # Errors
///
/// [`DataUseScanError::Io`] when `root` is not a readable directory.
pub fn lint_tree(root: &Path) -> Result<TreeLint, DataUseScanError> {
    if !root.is_dir() {
        return Err(DataUseScanError::Io {
            path: root.to_path_buf(),
            message: "not a readable directory".to_string(),
        });
    }
    let fallback = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .build();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                // Walker errors do not always carry a path; fall back to `root`.
                skipped.push((
                    root.to_path_buf(),
                    DataUseScanError::Io {
                        path: root.to_path_buf(),
                        message: e.to_string(),
                    },
                ));
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let in_root = path.strip_prefix(root).unwrap_or(path);
        if in_root.components().any(|c| c.as_os_str() == "target") || should_exclude_path(in_root) {
            continue;
        }
        files.push(path.to_path_buf());
    }
    files.sort();

    let mut rows = Vec::new();
    let mut names: HashMap<PathBuf, Option<String>> = HashMap::new();
    for path in files {
        let in_root = path.strip_prefix(root).unwrap_or(&path);
        let rel = in_root.to_string_lossy().replace('\\', "/");
        let crate_name =
            crate_name_for(&path, root, &mut names).unwrap_or_else(|| fallback.clone());
        match scan_file(&path, &crate_name, &rel) {
            Ok(found) => {
                let hits: Vec<ScanHit> = found.into_iter().map(hit_from_found_bare).collect();
                rows.extend(lint_scan_hits(&hits));
            }
            Err(e) => skipped.push((path, e)),
        }
    }
    Ok(TreeLint { rows, skipped })
}

fn hit_from_found_bare(f: crate::scan::FoundUse) -> ScanHit {
    crate::hit_from_found(f, "", &[])
}

/// `[package].name` of the nearest `Cargo.toml` from `file`'s directory up to `root`.
///
/// `names` caches each directory's answer (`None` = no named manifest there).
/// A manifest without a literal `name = "…"` (for example `name.workspace = true`)
/// is not a match, so the walk continues to the parent.
fn crate_name_for(
    file: &Path,
    root: &Path,
    names: &mut HashMap<PathBuf, Option<String>>,
) -> Option<String> {
    let mut dir = file.parent();
    while let Some(d) = dir {
        let found = names
            .entry(d.to_path_buf())
            .or_insert_with(|| {
                std::fs::read_to_string(d.join("Cargo.toml"))
                    .ok()
                    .and_then(|text| package_name(&text))
            })
            .clone();
        if found.is_some() {
            return found;
        }
        if d == root {
            break;
        }
        dir = d.parent();
    }
    None
}

/// The `[package]` table's literal `name = "…"`, ignoring trailing `#` comments.
fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line.split('#').next().map(str::trim) == Some("[package]");
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "name" {
            continue;
        }
        let value = value.split('#').next().unwrap_or("").trim();
        if let Some(name) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            return Some(name.to_string());
        }
    }
    None
}

/// Template and hedge hits per file, for the purpose rewrite fan-out.
///
/// A site is a hit when its gaps include [`GapCode::BanTemplate`] or
/// [`GapCode::Hedge`], and counts once however many phrases it contains.
/// `Display` prints one `<file>: <n>` line per file with a hit, then a total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitSummary {
    /// Hit sites per file, files with no hit omitted.
    pub by_file: BTreeMap<String, usize>,
    /// Sites with a template or hedge gap.
    pub hit_sites: usize,
    /// Files with at least one hit.
    pub hit_files: usize,
    /// All `use_!` sites linted.
    pub total_sites: usize,
}

impl HitSummary {
    /// Count hits in `rows`.
    #[must_use]
    pub fn from_rows(rows: &[InventoryRow]) -> Self {
        let mut by_file = BTreeMap::new();
        for row in rows {
            if row.gaps.iter().any(|g| g.is_template_or_hedge()) {
                *by_file.entry(row.file.clone()).or_insert(0) += 1;
            }
        }
        let hit_sites = by_file.values().sum();
        Self {
            hit_files: by_file.len(),
            by_file,
            hit_sites,
            total_sites: rows.len(),
        }
    }
}

impl fmt::Display for HitSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (file, n) in &self.by_file {
            writeln!(f, "{file}: {n}")?;
        }
        write!(
            f,
            "{} template/hedge hits in {} files; {} of {} use_! sites",
            self.hit_sites, self.hit_files, self.hit_sites, self.total_sites
        )
    }
}

/// CSV line (header via [`inventory_csv_header`]).
#[must_use]
pub fn inventory_csv_row(row: &InventoryRow) -> String {
    let gaps = row
        .gaps
        .iter()
        .map(|g| g.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let tier = match row.tier {
        PurposeTier::S3 => "S3",
        PurposeTier::S2 => "S2",
        PurposeTier::S1 => "S1",
        PurposeTier::S0 => "S0",
    };
    let score = if row.pass { "Pass" } else { "Fail" };
    format!(
        "{file}:{line},{crate},{method},{tier},{score},{gaps},{purpose}",
        file = csv_escape(&row.file),
        line = row.line,
        crate = csv_escape(&row.crate_name),
        method = csv_escape(&row.method),
        purpose = csv_escape(&row.purpose),
        gaps = csv_escape(&gaps),
    )
}

/// CSV header row.
#[must_use]
pub fn inventory_csv_header() -> &'static str {
    "file:line,crate,method,tier,score,gap_codes,purpose"
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
