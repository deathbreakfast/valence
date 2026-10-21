//! `lint_tree` and `HitSummary` over a temp checkout.

mod support;

use support::{file_with, site, write, CLEAN};
use valence_data_use_scan::{
    inventory_csv_row, lint_tree, DataUseScanError, GapCode, HitSummary, HEDGE_PHRASES,
    TEMPLATE_PHRASES,
};

fn banned_sites() -> Vec<String> {
    TEMPLATE_PHRASES
        .iter()
        .chain(HEDGE_PHRASES)
        .map(|p| site(&format!("We load the invoice list for the console. {p}.")))
        .collect()
}

#[test]
fn one_site_per_phrase_is_counted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sites = banned_sites();
    let expected = sites.len();
    assert_eq!(expected, TEMPLATE_PHRASES.len() + HEDGE_PHRASES.len());
    write(dir.path(), "app/src/bad.rs", &file_with(&sites));
    write(dir.path(), "app/src/good.rs", &file_with(&[site(CLEAN)]));

    let tree = lint_tree(dir.path()).expect("lint");
    assert!(tree.skipped.is_empty(), "{:?}", tree.skipped);
    let summary = HitSummary::from_rows(&tree.rows);
    assert_eq!(summary.by_file.len(), 1);
    assert_eq!(summary.by_file["app/src/bad.rs"], expected);
    assert_eq!(summary.hit_sites, expected);
    assert_eq!(summary.hit_files, 1);
    assert_eq!(summary.total_sites, expected + 1);
    assert_eq!(
        summary.to_string(),
        format!(
            "app/src/bad.rs: {expected}\n{expected} template/hedge hits in 1 files; {expected} of {} use_! sites",
            expected + 1
        )
    );
}

#[test]
fn double_phrase_site_counts_once_and_thin_site_is_csv_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        dir.path(),
        "src/a.rs",
        &file_with(&[
            site("We decide what to do next in this workflow, not necessarily shown, so saved changes apply on the next read."),
            site("Short purpose."),
        ]),
    );
    let tree = lint_tree(dir.path()).expect("lint");
    let summary = HitSummary::from_rows(&tree.rows);
    assert_eq!(summary.by_file["src/a.rs"], 1);
    assert_eq!(summary.total_sites, 2);
    let thin = tree
        .rows
        .iter()
        .find(|r| r.purpose == "Short purpose.")
        .expect("thin row present");
    assert!(thin.gaps.contains(&GapCode::TooThin));
    assert!(!thin.gaps.iter().any(|g| g.is_template_or_hedge()));
    assert!(inventory_csv_row(thin).contains("TOO_THIN"));
}

#[test]
fn skipped_paths_and_non_rust_files_contribute_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = file_with(&banned_sites());
    for rel in [
        "c/tests/it.rs",
        "c/target/debug/gen.rs",
        "c/src/foo_test.rs",
        "c/src/tests.rs",
    ] {
        write(dir.path(), rel, &bad);
    }
    write(dir.path(), "c/README.md", &bad);
    write(dir.path(), "c/notes.txt", &bad);
    let tree = lint_tree(dir.path()).expect("lint");
    assert!(tree.rows.is_empty(), "{:?}", tree.rows.len());
    assert!(tree.skipped.is_empty());
}

#[test]
fn unparsable_file_is_reported_and_others_still_linted() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/broken.rs", "fn {{{ not rust");
    write(dir.path(), "src/ok.rs", &file_with(&banned_sites()));
    let tree = lint_tree(dir.path()).expect("lint");
    assert_eq!(tree.skipped.len(), 1);
    assert!(tree.skipped[0].0.ends_with("src/broken.rs"));
    assert!(matches!(tree.skipped[0].1, DataUseScanError::Parse { .. }));
    assert!(HitSummary::from_rows(&tree.rows).hit_sites > 0);
}

#[test]
fn missing_root_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = lint_tree(&dir.path().join("nope")).expect_err("missing root");
    assert!(matches!(err, DataUseScanError::Io { .. }));
}

#[test]
fn crate_name_and_tier_come_from_the_tree() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        dir.path(),
        "lepton-auth/Cargo.toml",
        "[package]\nname = \"lepton-auth\"\nversion = \"0.1.0\"\n",
    );
    write(
        dir.path(),
        "lepton-auth/src/totp/api.rs",
        &file_with(&[site(CLEAN)]),
    );
    let tree = lint_tree(dir.path()).expect("lint");
    assert_eq!(tree.rows[0].crate_name, "lepton-auth");
    assert_eq!(tree.rows[0].file, "lepton-auth/src/totp/api.rs");
    assert_eq!(tree.rows[0].tier, valence_data_use_scan::PurposeTier::S3);
}
