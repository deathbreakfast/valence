//! Smoke-test record-history HistorySource / E2eHistorySourceA codegen.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;
use std::path::PathBuf;

use crate::codegen::{self, parser};

#[test]
fn generates_e2e_history_source_a_schema() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut trait_defs = HashMap::new();
    for name in [
        "history_source_valence_trait.rs",
        "record_history_valence_trait.rs",
    ] {
        let p = root.join(name);
        let c = std::fs::read_to_string(&p).expect("read trait");
        let d = parser::extract_trait_from_file(&c).expect("parse trait");
        trait_defs.insert(d.name.clone(), d);
    }
    let p = root.join("e2e_history_source_a_valence_schema.rs");
    let generated = codegen::generate_from_schema_file(&p, &trait_defs).expect("codegen");
    assert!(
        generated.contains("E2eHistorySourceA"),
        "expected E2eHistorySourceA in:\n{}",
        &generated[..generated.len().min(2000)]
    );
    assert!(
        generated.contains("label"),
        "expected label field accessors"
    );
}
