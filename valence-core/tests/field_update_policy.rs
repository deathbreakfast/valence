//! Field-level `update` policies are enforced on writes to an existing row.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use serde_json::json;
use valence_core::actor::Actor;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy_policies::common::SYSTEM_ONLY;
use valence_core::schema::SchemaMetadata;
use valence_core::schema_api::{
    Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
use valence_core::{DatabaseEvaluator, PrivacyEvaluator};

fn field(name: &str, system_only_update: bool) -> SchemaField {
    SchemaField {
        name: name.into(),
        field_type: "String".into(),
        primary: false,
        nullable: true,
        indexed: false,
        unique: false,
        default: None,
        fk: None,
        validations: vec![],
        policies: system_only_update.then(|| SchemaPolicies {
            update: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: "SYSTEM_ONLY".into(),
                    description: None,
                    evaluator: Some(&SYSTEM_ONLY),
                }],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        encrypted: false,
        enum_variants: vec![],
        enum_type: None,
        model_path: None,
    }
}

fn schema() -> &'static SchemaMetadata {
    let schema = Box::leak(Box::new(Schema {
        name: "fu_member".into(),
        version: "0.1.0".into(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "t".into(),
            write: "t".into(),
        },
        policies: None,
        fields: vec![field("role", false), field("added_by", true)],
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        retired_fields: Vec::new(),
        ttl: None,
        partition_export: None,
        ownership: None,
        partition_export: None,
        meta: SchemaMeta {
            retention: "1d".into(),
            row_count: 0,
            owner: "t".into(),
            description: None,
            repository: "https://github.com/unified-field-dev/valence".to_string(),
        },
    }));
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

fn user() -> Actor {
    Actor::User {
        user_id: "u1".into(),
    }
}

#[test]
fn user_rewriting_system_only_field_is_denied() {
    let before = json!({"role": "member", "added_by": "alice"});
    let after = json!({"role": "member", "added_by": "mallory"});
    let err = PrivacyEvaluator::check_field_update_access(schema(), Some(&before), &after, &user())
        .unwrap_err();
    assert!(err.to_string().contains("added_by"), "{err}");
}

#[test]
fn system_may_rewrite_system_only_field() {
    let before = json!({"added_by": "alice"});
    let after = json!({"added_by": "bob"});
    PrivacyEvaluator::check_field_update_access(
        schema(),
        Some(&before),
        &after,
        &Actor::System {
            operation: "test".into(),
        },
    )
    .unwrap();
}

#[test]
fn user_may_change_unrestricted_field_and_resend_unchanged_one() {
    let before = json!({"role": "member", "added_by": "alice"});
    let after = json!({"role": "admin", "added_by": "alice"});
    PrivacyEvaluator::check_field_update_access(schema(), Some(&before), &after, &user()).unwrap();
}

#[test]
fn absent_and_null_count_as_unchanged() {
    let before = json!({"role": "member"});
    let after = json!({"role": "admin", "added_by": null});
    PrivacyEvaluator::check_field_update_access(schema(), Some(&before), &after, &user()).unwrap();
}

#[test]
fn create_without_before_is_not_checked() {
    let after = json!({"added_by": "alice"});
    PrivacyEvaluator::check_field_update_access(schema(), None, &after, &user()).unwrap();
}
