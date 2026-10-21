//! The `valence-purpose-lint` binary end to end.

mod support;

use std::path::Path;
use std::process::{Command, Output};

use support::{file_with, site, write, CLEAN};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_valence-purpose-lint"))
        .args(args)
        .output()
        .expect("run binary")
}

fn s(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

fn fixture(root: &Path) {
    write(
        root,
        "src/bad.rs",
        &file_with(&[
            site("We decide what to do next in this workflow for the operator console page."),
            site("It is not necessarily displayed on a page unless that feature's UI shows it."),
        ]),
    );
    write(root, "src/good.rs", &file_with(&[site(CLEAN)]));
}

#[test]
fn reports_hits_per_file_and_exits_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    fixture(dir.path());
    let out = run(&[dir.path().to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        s(&out.stdout),
        "src/bad.rs: 2\n2 template/hedge hits in 1 files; 2 of 3 use_! sites\n"
    );
}

#[test]
fn two_roots_are_aggregated() {
    let a = tempfile::tempdir().expect("tempdir");
    let b = tempfile::tempdir().expect("tempdir");
    fixture(a.path());
    fixture(b.path());
    let out = run(&[
        a.path().to_str().expect("utf8"),
        b.path().to_str().expect("utf8"),
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(s(&out.stdout).ends_with("4 template/hedge hits in 1 files; 4 of 6 use_! sites\n"));
}

#[test]
fn empty_directory_has_no_hits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run(&[dir.path().to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        s(&out.stdout),
        "0 template/hedge hits in 0 files; 0 of 0 use_! sites\n"
    );
}

#[test]
fn missing_path_and_no_args_exit_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("nope");
    let out = run(&[missing.to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        s(&out.stderr).contains("not a readable directory"),
        "{}",
        s(&out.stderr)
    );

    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(s(&out.stderr).contains("usage:"));
    assert_eq!(run(&["--bogus", "x"]).status.code(), Some(2));
}

#[test]
fn unparsable_file_exits_one_but_still_counts() {
    let dir = tempfile::tempdir().expect("tempdir");
    fixture(dir.path());
    write(dir.path(), "src/broken.rs", "fn {{{ not rust");
    let out = run(&[dir.path().to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(s(&out.stderr).contains("skipped"));
    assert!(s(&out.stdout).contains("src/bad.rs: 2"));
}

#[test]
fn sites_and_csv_modes() {
    let dir = tempfile::tempdir().expect("tempdir");
    fixture(dir.path());
    let root = dir.path().to_str().expect("utf8");
    let out = s(&run(&["--sites", root]).stdout);
    assert!(
        out.contains("src/bad.rs:2: BAN_TEMPLATE decide what to do next in this workflow"),
        "{out}"
    );
    assert!(
        out.contains("HEDGE not necessarily; unless that feature"),
        "{out}"
    );

    let out = s(&run(&["--csv", root]).stdout);
    assert!(out.starts_with("file:line,crate,method,tier,score,gap_codes,purpose\n"));
    assert!(out.contains("src/good.rs:"));
}

#[test]
fn sites_pairs_each_code_with_its_own_phrases() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        dir.path(),
        "src/both.rs",
        &file_with(&[site(
            "We decide what to do next in this workflow, not necessarily shown to the operator.",
        )]),
    );
    let out = s(&run(&["--sites", dir.path().to_str().expect("utf8")]).stdout);
    assert!(
        out.contains("src/both.rs:2: BAN_TEMPLATE decide what to do next in this workflow\n"),
        "{out}"
    );
    assert!(
        out.contains("src/both.rs:2: HEDGE not necessarily\n"),
        "{out}"
    );
}
