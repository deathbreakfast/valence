//! Shared fixture builders for the tree and CLI tests.
#![allow(dead_code)]

use std::fs;
use std::path::Path;

/// One `use_!` call site in an async fn body.
pub fn site(purpose: &str) -> String {
    format!("    let _ = Thing::get(\"id\", &valence, valence::use_!(r#\"{purpose}\"#)).await;\n")
}

pub fn file_with(sites: &[String]) -> String {
    format!("async fn f() {{\n{}}}\n", sites.concat())
}

pub fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, body).expect("write");
}

pub const CLEAN: &str = "When an operator opens the **Applications** page, we **load each application's name, image and desired instance count** so the table can list them. Only operators with access to that console see this list.";
