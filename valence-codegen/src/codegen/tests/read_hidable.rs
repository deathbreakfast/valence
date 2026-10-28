//! Codegen shape for required fields that carry their own `read` policy.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

fn generated() -> String {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Profile {
        repository: "https://github.com/unified-field-dev/valence",
        table: "profile",
        version: "0.1.0",
        policies: { read: { allow: [valence::privacy_policies::common::AUTHENTICATED] } },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
            legal_name: {
                r#type: FieldType::String,
                required: true,
                policies: { read: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] } },
            },
            display_name: { r#type: FieldType::String, required: true },
        ]
    }
}
"#;
    let path = write_temp_schema_file(schema, "read_hidable_schema.rs");
    let out = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");
    out.split_whitespace().collect()
}

#[test]
fn read_hidable_required_field_is_optional_on_read_and_required_on_create() {
    let g = generated();
    assert!(g.contains("legal_name:Option<String>"), "storage is Option");
    assert!(
        g.contains("pubfnlegal_name(&self)->Option<&String>"),
        "getter is Option"
    );
    assert!(
        g.contains("legal_name:String,"),
        "constructor takes the bare value"
    );
    assert!(
        g.contains("legal_name:Some(legal_name)"),
        "constructor stores Some"
    );
    assert!(
        g.contains(r#"skip_serializing_if="Option::is_none""#),
        "a hidden value is never written as null"
    );
    assert!(
        g.contains(r#"fn__read_hidable_required_fields()->&'static[&'staticstr]{&["legal_name"]}"#),
        "write guards list the field"
    );
}

#[test]
fn field_without_read_policy_keeps_required_shape() {
    let g = generated();
    assert!(g.contains("display_name:String"));
    assert!(g.contains("pubfndisplay_name(&self)->&String"));
    assert!(
        !g.contains("display_name:Some(display_name)"),
        "constructor stores the bare value"
    );
}

#[test]
fn public_read_field_policy_keeps_required_shape() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Card {
        repository: "https://github.com/unified-field-dev/valence",
        table: "card",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: {
                r#type: FieldType::String,
                required: true,
                policies: { read: { allow: [valence::privacy_policies::common::PUBLIC_READ] } },
            },
        ]
    }
}
"#;
    let path = write_temp_schema_file(schema, "public_read_schema.rs");
    let g: String = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen")
        .split_whitespace()
        .collect();
    assert!(
        g.contains("pubfntitle(&self)->&String"),
        "PUBLIC_READ never hides the field"
    );
    assert!(g.contains(r"fn__read_hidable_required_fields()->&'static[&'staticstr]{&[]}"));
}

#[test]
fn schema_cannot_hide_a_field_its_trait_requires() {
    let trait_src = r#"
use valence::prelude::*;

valence_trait_schema! {
    Tokened {
        repository: "https://github.com/unified-field-dev/valence",
        fields: [
            token_hash: { r#type: FieldType::String, required: true },
        ],
    }
}
"#;
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    ResetToken {
        repository: "https://github.com/unified-field-dev/valence",
        table: "reset_token",
        version: "0.1.0",
        traits: [Tokened],
        policies: { read: { allow: [valence::privacy_policies::common::AUTHENTICATED] } },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            token_hash: {
                r#type: FieldType::String,
                required: true,
                policies: { read: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] } },
            },
        ]
    }
}
"#;
    let trait_def = crate::codegen::parser::extract_trait_from_file(trait_src).expect("trait");
    let mut trait_defs = HashMap::new();
    trait_defs.insert(trait_def.name.clone(), trait_def);
    let path = write_temp_schema_file(schema, "trait_conflict_schema.rs");
    let err = generate_from_schema_file(&path, &trait_defs)
        .expect_err("a required trait field can't be hidden by one implementor")
        .to_string();
    assert!(err.contains("reset_token.token_hash"), "{err}");
    assert!(
        err.contains("Declare the read policy on the trait field"),
        "{err}"
    );
}

#[test]
fn field_read_matching_the_row_read_keeps_required_shape() {
    // Only System reads the row, and System passes the field rule: nobody who sees the
    // row ever loses the field, so it stays a plain required value.
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Secret {
        repository: "https://github.com/unified-field-dev/valence",
        table: "secret",
        version: "0.1.0",
        policies: { read: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] } },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            token_hash: {
                r#type: FieldType::String,
                required: true,
                policies: { read: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] } },
            },
        ]
    }
}
"#;
    let path = write_temp_schema_file(schema, "redundant_field_read_schema.rs");
    let g: String = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen")
        .split_whitespace()
        .collect();
    assert!(
        g.contains("pubfntoken_hash(&self)->&String"),
        "stays required"
    );
}
