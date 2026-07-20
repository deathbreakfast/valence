//! Parser integration tests using the shared syn DSL.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::extract_schema_from_macro;

#[test]
fn parses_ttl_from_schema_file() {
    let schema = extract_schema_from_macro(
        r#"
        valence_schema! {
            Foo {
                table: "foo",
                version: "0.1.0",
                ttl: { seconds: 1800 },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                ],
            }
        }
        "#,
    )
    .expect("parse")
    .schema;
    let ttl = schema.ttl.expect("ttl");
    assert_eq!(ttl.seconds, 1800);
}

#[test]
fn parses_database_expr() {
    let parsed = extract_schema_from_macro(
        r#"
        use crate::PROJECT_DB;
        valence_schema! {
            Project {
                table: "project",
                version: "0.1.0",
                database: PROJECT_DB,
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                ],
            }
        }
        "#,
    )
    .expect("parse");
    assert!(parsed.database.is_some());
}

#[test]
fn parses_side_effects_list() {
    let schema = extract_schema_from_macro(
        r#"
        valence_schema! {
            Foo {
                table: "foo",
                version: "0.1.0",
                side_effects: [MyEffect],
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                ],
            }
        }
        "#,
    )
    .expect("parse")
    .schema;
    assert_eq!(schema.side_effects, vec!["MyEffect"]);
}

#[test]
fn parses_trait_target_connection() {
    let schema = extract_schema_from_macro(
        r#"
        valence_schema! {
            Foo {
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                ],
                connections: [
                    owner: {
                        target_trait: "HasOwner",
                        cardinality: HasOne,
                        on_delete: Cascade,
                    },
                ],
            }
        }
        "#,
    )
    .expect("parse")
    .schema;
    assert_eq!(schema.connections.len(), 1);
    assert_eq!(
        schema.connections[0].target_trait.as_deref(),
        Some("HasOwner")
    );
    assert_eq!(schema.connections[0].to_table, "trait:HasOwner");
}
