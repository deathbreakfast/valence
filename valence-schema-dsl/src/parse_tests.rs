//! Unit tests for [`SchemaSpec`] parsing and [`SchemaSpec::to_schema`] lowering.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::manual_let_else)]

use crate::parse::*;

#[test]
fn test_parse_minimal_schema() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                    name: { r#type: FieldType::String, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "foo");
    assert_eq!(parsed.version, "0.1.0");
    assert_eq!(parsed.fields.len(), 2);
}

#[test]
fn test_parse_connections_block() {
    let input = r#"
            Post {
                table: "post",
                version: "0.1.0",
                fields: [ id: { r#type: FieldType::String, primary_key: true, required: true } ],
                connections: [
                    author: { table: "user", on_delete: Cascade, model: "my_crate::User" }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "post");
    assert_eq!(parsed.connections.len(), 1);
    assert_eq!(parsed.connections[0].name, "author");
    assert_eq!(parsed.connections[0].table, "user");
    assert_eq!(
        parsed.connections[0].model.as_deref(),
        Some("my_crate::User")
    );
    assert_eq!(parsed.fields.len(), 1);
}

#[test]
fn test_parse_rejects_unknown_key() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                unknown_key: 1,
                fields: []
            }
        "#;
    let result = syn::parse_str::<SchemaSpec>(input);
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected parse error for unknown key"),
    };
    assert!(err.to_string().contains("Unknown schema key"));
}

#[test]
fn test_parse_traits_key() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ],
                traits: [Named, HasFiles]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "foo");
    assert_eq!(parsed.traits, vec!["Named", "HasFiles"]);
}

#[test]
fn test_parse_ttl_block() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                ttl: {
                    seconds: 1800,
                },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    let ttl = parsed.ttl.expect("ttl should parse");
    assert_eq!(ttl.seconds, 1800);
    assert_eq!(ttl.mode, "backend_capability");
}

#[test]
fn test_parse_enum_field_type() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                    phase: {
                        r#type: FieldType::Enum(&["PENDING", "IN_PROGRESS", "COMPLETED"]),
                        required: true,
                    }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.fields.len(), 2);
    assert_eq!(parsed.fields[1].name, "phase");
    assert_eq!(
        parsed.fields[1].field_type,
        "enum:PENDING,IN_PROGRESS,COMPLETED"
    );
}

#[test]
fn test_parse_external_enum_field_type() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                    color: {
                        r#type: FieldType::ExternalEnum("crate::ColorEnum"),
                        required: true,
                    }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.fields.len(), 2);
    assert_eq!(parsed.fields[1].name, "color");
    assert_eq!(parsed.fields[1].field_type, "ext_enum:crate::ColorEnum");
}

#[test]
fn test_parse_connections_optional() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "foo");
    assert_eq!(parsed.fields.len(), 1);
    assert!(parsed.connections.is_empty());
}

#[test]
fn test_parse_duplicate_database() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                database: DB_A,
                database: DB_B,
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("duplicate `database:`"));
}

#[test]
fn test_parse_ownership_mutual_exclusion() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                ownership: { system: true, resolve: crate::Resolver },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("ownership:"));
}

#[test]
fn test_parse_ttl_requires_seconds() {
    let input = r#"
            Foo {
                table: "foo",
                version: "0.1.0",
                ttl: { mode: "backend_capability" },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("ttl.seconds"));
}

#[test]
fn test_parse_connection_requires_on_delete() {
    let input = r#"
            Post {
                table: "post",
                version: "0.1.0",
                fields: [ id: { r#type: FieldType::String, primary_key: true, required: true } ],
                connections: [
                    author: { table: "user" }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("on_delete"));
}

#[test]
fn test_parse_missing_table() {
    let input = r#"
            Foo {
                version: "0.1.0",
                fields: []
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("Missing 'table'"));
}
