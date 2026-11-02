//! `SchemaRegistry::auto_discover()` exposes `SchemaMetadata.databases` for macro-declared schemas.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
#![cfg(feature = "surreal-inventory")]

mod support;

use valence::SchemaRegistry;

#[test]
fn auto_discover_exposes_databases_for_macro_schema() {
    let registry = SchemaRegistry::auto_discover();
    let meta = registry
        .get_schema("e2e_inventory_smoke")
        .expect("macro schema discovered through inventory");
    assert_eq!(meta.databases, ["e2e_inventory".to_string()]);
    let full = registry
        .get_full_schema("e2e_inventory_smoke")
        .expect("full schema");
    assert_eq!(full.databases, meta.databases);
    assert!(registry.list_schemas().contains(&"e2e_inventory_smoke"));
}

#[test]
fn auto_discover_has_no_databases_for_unknown_table() {
    let registry = SchemaRegistry::auto_discover();
    assert!(registry.get_schema("no_such_table").is_none());
    assert!(!registry.has_schema("no_such_table"));
    assert!(SchemaRegistry::new().list_schemas().is_empty());
}
