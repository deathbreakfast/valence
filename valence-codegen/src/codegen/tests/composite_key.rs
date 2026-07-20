#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

#[test]
fn test_composite_key_codegen() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestCheckpoint {
        table: "test_checkpoint",
        version: "0.1.0",
        description: "Test entity with composite key",

        composite_key: [owner_name, category],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            owner_name: { r#type: FieldType::String, required: true },
            category: { r#type: FieldType::String, required: true },
            score: { r#type: FieldType::Integer, required: true },
            note: { r#type: FieldType::String, required: false },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "composite_key_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("composite key codegen failed");

    // composite_id method generated
    assert!(
        generated.contains("fn composite_id"),
        "Expected composite_id method"
    );

    // get_by_composite_key method generated
    assert!(
        generated.contains("fn get_by_composite_key"),
        "Expected get_by_composite_key method"
    );

    // upsert_by_composite_key method generated
    assert!(
        generated.contains("fn upsert_by_composite_key"),
        "Expected upsert_by_composite_key method"
    );

    // Composite key fields should NOT have setters in the Mutable
    assert!(
        !generated.contains("fn set_owner_name"),
        "Composite key field owner_name should NOT have a setter"
    );
    assert!(
        !generated.contains("fn set_category"),
        "Composite key field category should NOT have a setter"
    );

    // Non-composite fields should still have setters
    assert!(
        generated.contains("fn set_score"),
        "Non-composite field score should have a setter"
    );
    assert!(
        generated.contains("fn set_note"),
        "Non-composite field note should have a setter"
    );

    // Query method: where_owner_name_and_category
    assert!(
        generated.contains("fn where_owner_name_and_category"),
        "Expected where_owner_name_and_category composite query method"
    );
}

#[test]
fn test_composite_key_with_optional_field() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestPartialKey {
        table: "test_partial_key",
        version: "0.1.0",

        composite_key: [region, sub_key],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            region: { r#type: FieldType::String, required: true },
            sub_key: { r#type: FieldType::String, required: false },
            value: { r#type: FieldType::Integer, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "composite_key_optional_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("composite key with optional field codegen failed");

    assert!(
        generated.contains("fn composite_id"),
        "Expected composite_id method"
    );
    assert!(
        generated.contains("fn where_region_and_sub_key"),
        "Expected where_region_and_sub_key composite query method"
    );
    // sub_key is optional, so composite_id should handle __null__
    assert!(
        generated.contains("__null__"),
        "Expected __null__ sentinel for optional composite key field"
    );
}

#[test]
fn test_composite_key_validation_rejects_unknown_field() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    BadComposite {
        table: "bad_composite",
        version: "0.1.0",

        composite_key: [owner_name, nonexistent_field],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            owner_name: { r#type: FieldType::String, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "composite_key_bad_field_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_err(),
        "Expected codegen to fail when composite_key references unknown field"
    );
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("nonexistent_field"),
        "Error should mention the unknown field: {err_str}"
    );
}

#[test]
fn test_composite_key_validation_rejects_primary_key() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    BadPkComposite {
        table: "bad_pk_composite",
        version: "0.1.0",

        composite_key: [id, name],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "composite_key_pk_field_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_err(),
        "Expected codegen to fail when composite_key includes primary key"
    );
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("primary key"),
        "Error should mention primary key: {err_str}"
    );
}
