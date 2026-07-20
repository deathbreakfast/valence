//! Emit-parity checks: codegen metadata must honor DSL `database:`, policies, ownership, composite_key.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

#[test]
fn metadata_emits_database_evaluator_path() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Routed {
        table: "routed",
        version: "0.1.0",
        database: crate::ROUTED_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ],
    }
}
"#;
    let path = write_temp_schema_file(schema, "emit_parity_db.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");
    assert!(
        generated.contains("&crate::ROUTED_DB"),
        "expected database evaluator path in metadata"
    );
    assert!(
        !generated.contains("postgres-main"),
        "must not hardcode postgres-main databases list"
    );
    assert!(
        generated.contains("__valence_db_eval.name()"),
        "databases list should come from evaluator.name()"
    );
}

#[test]
fn metadata_emits_ownership_and_composite_key() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    OwnedComp {
        table: "owned_comp",
        version: "0.1.0",
        ownership: { system: true },
        composite_key: [tenant, slug],
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            tenant: { r#type: FieldType::String, required: true },
            slug: { r#type: FieldType::String, required: true },
        ],
    }
}
"#;
    let path = write_temp_schema_file(schema, "emit_parity_own.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");
    assert!(
        generated.contains("system_owned: true"),
        "ownership must be emitted"
    );
    assert!(
        generated.contains("\"tenant\"") && generated.contains("\"slug\""),
        "composite_key fields must appear in metadata"
    );
    assert!(
        !generated.contains("composite_key: Vec::new()"),
        "composite_key must not be forced empty"
    );
}

#[test]
fn metadata_emits_policy_evaluators() {
    let schema = r#"
use valence::prelude::*;
use valence::privacy_policies::common::PUBLIC_READ;

valence_schema! {
    PolicyProbe {
        table: "policy_probe",
        version: "0.1.0",
        policies: {
            read: { allow: [PUBLIC_READ] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ],
    }
}
"#;
    let path = write_temp_schema_file(schema, "emit_parity_pol.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");
    assert!(
        generated.contains("Box::leak") && generated.contains("PolicyEvaluator"),
        "policy rules must leak evaluators like the macro path"
    );
    assert!(
        !generated.contains("evaluator: None"),
        "policy evaluators must not be None"
    );
}

#[test]
fn rejects_string_literal_database() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    BadDb {
        table: "bad_db",
        version: "0.1.0",
        database: "not-an-evaluator",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ],
    }
}
"#;
    let path = write_temp_schema_file(schema, "emit_parity_bad_db.rs");
    let err = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect_err("string database should fail");
    assert!(
        err.to_string().contains("string literal"),
        "unexpected error: {err}"
    );
}
