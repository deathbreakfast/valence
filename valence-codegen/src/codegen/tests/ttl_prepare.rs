#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

#[test]
fn create_and_creating_upsert_call_prepare_create_content() {
    let schema = r#"
use valence::prelude::*;
use valence::privacy_policies::common::{PUBLIC_READ, AUTHENTICATED};

valence_schema! {
    TtlPrepareProbe {
        table: "ttl_prepare_probe",
        database: valence::DEFAULT_SURREAL_STORAGE,
        version: "0.1.0",
        description: "codegen TTL prepare probe",
        ttl: { seconds: 1800 },

        policies: {
            read: { allow: [PUBLIC_READ] },
            create: { allow: [AUTHENTICATED] },
            update: { allow: [AUTHENTICATED] },
        },

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "ttl_prepare_probe.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");

    assert!(
        generated.contains("prepare_create_content"),
        "Model::create / upsert(create) must call prepare_create_content"
    );
    assert!(
        generated.contains("if creating"),
        "upsert must gate prepare on creating path only"
    );
}
