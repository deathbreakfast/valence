#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

#[test]
fn generates_code_from_dsl_schema() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestEntity {
        table: "test_entity",
        version: "0.1.0",
        description: "Test entity for DSL parsing",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
            deleted_at: { r#type: FieldType::Datetime, required: false },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "dsl_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("DSL codegen failed");
    insta::assert_snapshot!(generated);
}

#[test]
fn generates_clear_method_for_optional_fields() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestNotification {
        table: "test_notification",
        version: "0.1.0",
        description: "Test entity with optional field for clear method",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            message: { r#type: FieldType::String, required: true },
            read_at: { r#type: FieldType::Datetime, required: false },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "optional_field_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen failed");

    // Verify clear_read_at method is generated
    assert!(
        generated.contains("clear_read_at"),
        "Expected clear_read_at method to be generated"
    );
    assert!(
        generated.contains("pub fn clear_read_at"),
        "Expected clear_read_at to be a public method"
    );

    // Verify the mutable field uses Option<Option<T>> for optional fields
    assert!(
        generated.contains("Option<Option<chrono::DateTime<chrono::Utc>>>"),
        "Expected Option<Option<T>> for optional datetime field"
    );

    // Verify set method uses Some(Some(value))
    assert!(
        generated.contains("Some(Some(value))"),
        "Expected setter to use Some(Some(value)) for optional fields"
    );

    insta::assert_snapshot!(generated);
}

#[test]
fn generates_field_changes_and_dispatch_for_side_effects() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    ScoreTracker {
        table: "score_tracker",
        version: "0.1.0",
        description: "Test entity with side effects",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::Record("user"), required: true },
            value: { r#type: FieldType::Integer, required: true },
        ],

        side_effects: [ScoreNotifier, AuditLogger],
    }
}
"#;

    let path = write_temp_schema_file(schema, "side_effects_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen failed");

    // FieldChanges struct is generated
    assert!(
        generated.contains("pub struct ScoreTrackerFieldChanges"),
        "Expected ScoreTrackerFieldChanges struct to be generated"
    );

    // FieldChanges has typed fields for non-PK fields
    assert!(
        generated.contains("pub value: valence::FieldChange<i64>"),
        "Expected typed FieldChange<i64> for value field"
    );
    assert!(
        generated.contains("pub user: valence::FieldChange<valence::RecordId>"),
        "Expected typed FieldChange<RecordId> for user field"
    );

    // FieldChanges associated type is set on Model
    assert!(
        generated.contains("type FieldChanges = ScoreTrackerFieldChanges"),
        "Expected FieldChanges associated type on Model impl"
    );

    // Side effect dispatch references both registered types
    assert!(
        generated.contains("Box::new(ScoreNotifier)"),
        "Expected ScoreNotifier instantiation in dispatch"
    );
    assert!(
        generated.contains("Box::new(AuditLogger)"),
        "Expected AuditLogger instantiation in dispatch"
    );
    assert!(
        generated.contains("dyn valence::SideEffect<Self>"),
        "Expected SideEffect trait object in dispatch"
    );

    // Mutation construction in create
    assert!(
        generated.contains("valence::MutationKind::Create"),
        "Expected MutationKind::Create in generated code"
    );

    // Mutation construction in update
    assert!(
        generated.contains("valence::MutationKind::Update"),
        "Expected MutationKind::Update in generated code"
    );

    // Queued delete path (physical delete runs in host deletion worker)
    assert!(
        generated.contains("valence::deletion::DeletionService::create_run"),
        "Expected queued deletion run creation in generated delete()"
    );

    // Side effect errors are logged, not propagated
    assert!(
        generated.contains("record_side_effect_error"),
        "Expected side-effect error instrumentation in dispatch"
    );

    insta::assert_snapshot!(generated);
}

#[test]
fn generates_no_op_dispatch_without_side_effects() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    PlainModel {
        table: "plain_model",
        version: "0.1.0",
        description: "Model with no side effects",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "no_side_effects_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen failed");

    // FieldChanges struct is still generated
    assert!(
        generated.contains("pub struct PlainModelFieldChanges"),
        "Expected PlainModelFieldChanges struct"
    );

    // No-op dispatch: has unused_variables allow and empty body
    assert!(
        generated.contains("#[allow(unused_variables)]"),
        "Expected #[allow(unused_variables)] on no-op dispatch"
    );

    // Should NOT contain side-effect error instrumentation (no-op path)
    assert!(
        !generated.contains("record_side_effect_error"),
        "No-op dispatch should not contain side-effect error instrumentation"
    );

    // Should NOT contain Box::new (no side effects to instantiate)
    assert!(
        !generated.contains("Box::new"),
        "No-op dispatch should not contain side effect instantiations"
    );

    insta::assert_snapshot!(generated);
}

#[test]
fn rejects_toml_schema() {
    let schema = r##"
use valence::prelude::*;

valence_schema! {
    r#"
[table_properties]
name = "toml_entity"
version = "1.0.0"
description = "TOML based schema"

[privacy]
gdpr_compliant = false
deletion_tracking = true
roles = ["public"]

[mutation]
create_roles = ["system", "user"]
update_roles = ["system", "user"]
delete_roles = ["system", "admin"]

[[fields]]
name = "id"
type = "string"
primary_key = true
required = true

[[fields]]
name = "count"
type = "integer"
required = true
default = "0"
validations = ["non_negative"]
"#
}
"##;

    let path = write_temp_schema_file(schema, "toml_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(result.is_err());
}

#[test]
fn test_union_join_methods_generated() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestEntity {
        table: "test_entity",
        version: "0.1.0",
        description: "Simple entity for union/join test",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "union_join_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("union/join codegen failed");

    assert!(
        generated.contains("fn union"),
        "Expected union method to be generated on query builder"
    );
    assert!(
        generated.contains("fn join"),
        "Expected join method to be generated on query builder"
    );
    assert!(
        generated.contains("union_with"),
        "Expected union_with call in generated union method"
    );
    assert!(
        generated.contains("join_with"),
        "Expected join_with call in generated join method"
    );
}
