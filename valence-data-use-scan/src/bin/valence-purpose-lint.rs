//! Count template and hedge hits in `use_!` purposes across one or more checkouts.
//!
//! For contributors rewriting purposes. A hit is a `use_!` site whose purpose
//! carries `BAN_TEMPLATE` (a migration template sentence) or `HEDGE` ("not
//! necessarily", "unless that feature"). To fix one, follow the result to its
//! consumer and say what data is read or written, why, and which product
//! feature serves it. Other gaps, such as `TOO_THIN`, show only in `--csv`.
//!
//! Exit codes: 0 on success, even with hits (inventory, not a gate); 1 if any
//! file could not be read or parsed; 2 on a usage error or an unreadable root;
//! 3 when `--deny` is set and there is at least one hit. A skipped file wins
//! over 3, so a tree that was not fully read never passes as clean.

use std::path::PathBuf;
use std::process::ExitCode;

use valence_data_use_scan::{
    inventory_csv_header, inventory_csv_row, lint_tree, matching_phrases_for, HitSummary,
};

const USAGE: &str = "usage: valence-purpose-lint [--sites | --csv] [--deny] <path>...\n\n\
Counts use_! purposes that carry a migration template phrase (BAN_TEMPLATE) or a\n\
hedge (HEDGE), per file, across each checkout path.\n\n\
  --sites  also print <file>:<line>: <GAP_CODE> <matched phrase> for every hit\n\
  --csv    print every site with all gap codes instead of the summary\n\
  --deny   exit 3 when there is any hit, for CI gates\n\n\
Exit codes: 0 ok (hits do not fail the run unless --deny), 1 a file was skipped,\n\
2 usage or path error, 3 hits found with --deny.";

fn main() -> ExitCode {
    let mut sites = false;
    let mut csv = false;
    let mut deny = false;
    let mut roots = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--sites" => sites = true,
            "--csv" => csv = true,
            "--deny" => deny = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            a if a.starts_with("--") => {
                eprintln!("unknown option `{a}`\n{USAGE}");
                return ExitCode::from(2);
            }
            _ => roots.push(PathBuf::from(arg)),
        }
    }
    if roots.is_empty() || (sites && csv) {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let mut rows = Vec::new();
    let mut any_skipped = false;
    for root in &roots {
        match lint_tree(root) {
            Ok(tree) => {
                for (path, err) in &tree.skipped {
                    eprintln!("skipped {}: {err}", path.display());
                    any_skipped = true;
                }
                rows.extend(tree.rows);
            }
            Err(e) => {
                eprintln!("{}: {e}", root.display());
                return ExitCode::from(2);
            }
        }
    }

    let hit_sites = HitSummary::from_rows(&rows).hit_sites;
    if csv {
        println!("{}", inventory_csv_header());
        for row in &rows {
            println!("{}", inventory_csv_row(row));
        }
    } else {
        println!("{}", HitSummary::from_rows(&rows));
        if sites {
            for row in &rows {
                for gap in row.gaps.iter().filter(|g| g.is_template_or_hedge()) {
                    let phrases = matching_phrases_for(&row.purpose, *gap).join("; ");
                    println!("{}:{}: {gap} {phrases}", row.file, row.line);
                }
            }
        }
    }
    if any_skipped {
        ExitCode::from(1)
    } else if deny && hit_sites > 0 {
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    }
}
