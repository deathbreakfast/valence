//! Field `read` policies must hold sync rules only; codegen emits a type assertion per rule.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

fn generated(entity_read: &str, field_policies: &str) -> String {
    let schema = format!(
        r#"
use valence::prelude::*;

valence_schema! {{
    Profile {{
        repository: "https://github.com/unified-field-dev/valence",
        table: "profile",
        version: "0.1.0",
        policies: {{ read: {{ {entity_read} }} }},
        fields: [
            id: {{ r#type: FieldType::String, primary_key: true, required: true }},
            git_url: {{
                r#type: FieldType::String,
                required: true,
                policies: {{ read: {{ {field_policies} }} }},
            }},
        ]
    }}
}}
"#
    );
    let path = write_temp_schema_file(&schema, "field_policy_sync_rule_schema.rs");
    let out = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");
    out.split_whitespace().collect()
}

#[test]
fn field_read_rules_get_a_sync_assertion_naming_table_field_and_bucket() {
    let g = generated(
        "allow: [valence::privacy_policies::common::AUTHENTICATED]",
        "allow: [GLUON_APP_CHILD_RESOURCE], block: [SOME_BLOCK]",
    );
    assert!(g.contains("fnfield_read_rule_must_be_sync__profile__git_url__allow_0()"));
    assert!(g.contains("fnfield_read_rule_must_be_sync__profile__git_url__block_0()"));
    assert!(g.contains("must_be_sync_privacy_rule(&GLUON_APP_CHILD_RESOURCE)"));
}

#[test]
fn entity_read_rules_get_no_sync_assertion() {
    let g = generated(
        "allow: [valence::privacy_policies::common::AUTHENTICATED]",
        "allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD]",
    );
    assert_eq!(g.matches("must_be_sync_privacy_rule(&").count(), 1);
}
