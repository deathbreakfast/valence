//! Inventory ledger: every scanned `use_!` with purpose lint score.

use crate::lint_purpose::{lint_purpose, GapCode, PurposeTier};
use crate::ScanHit;

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
