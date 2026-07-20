#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

#[test]
fn test_connection_missing_table_rejected() {
    // Connection with model but no table: should fail codegen
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Post {
        table: "post",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            author: { r#type: FieldType::Record("user"), required: true },
        ],
        connections: [
            author: { model: "my_crate::User" },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "connection_missing_table_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_err(),
        "Expected codegen to fail when connection misses table:"
    );
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("missing table") || err_str.contains("table:"),
        "Error should mention missing table: {err_str}"
    );
}
#[test]
fn test_fk_without_connection_rejected() {
    // Record field (FK) but no matching connection in explicit connections block
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    OrphanFk {
        table: "orphan_fk",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            owner: { r#type: FieldType::Record("user"), required: true },
        ],
        connections: [
            other: { table: "other", cardinality: HasMany, reverse_field: "parent", on_delete: Cascade, model: "crate::Other" },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "fk_without_connection_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_err(),
        "Expected codegen to fail when FK has no matching connection"
    );
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("owner")
            && (err_str.contains("no matching connection") || err_str.contains("connection")),
        "Error should mention owner and connection: {err_str}"
    );
}

#[test]
fn test_hasone_connection_without_fk_rejected() {
    // HasOne connection declared but no Record field (no FK on this table)
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    MissingFk {
        table: "missing_fk",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
        connections: [
            author: {
                table: "user",
                cardinality: HasOne,
                on_delete: Cascade,
                model: "crate::User",
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "hasone_without_fk_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_err(),
        "Expected codegen to fail when HasOne connection has no FK field"
    );
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("author") && (err_str.contains("HasOne") || err_str.contains("Record")),
        "Error should mention author and HasOne: {err_str}"
    );
}

#[test]
fn test_valid_fk_and_connection_passes() {
    // Record field with matching connection - should succeed
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    ValidLinked {
        table: "valid_linked",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            author: { r#type: FieldType::Record("user"), required: true },
        ],
        connections: [
            author: { table: "user", on_delete: Cascade, model: "crate::User" },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "valid_fk_connection_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_ok(),
        "Valid FK+connection should succeed: {:?}",
        result.err()
    );
    let generated = result.unwrap();
    assert!(
        generated.contains("get_author"),
        "Expected get_author method"
    );
}
