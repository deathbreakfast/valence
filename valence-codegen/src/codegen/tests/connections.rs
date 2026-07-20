#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

#[test]
fn test_connections_dsl_parses() {
    // Schema with explicit connections: block (no Record in fields)
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Project {
        table: "project",
        version: "0.1.0",
        description: "Project with explicit connections",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
            owner: { r#type: FieldType::Record("user"), required: true },
        ],
        connections: [
            owner: {
                table: "user",
                cardinality: HasOne,
                required: true,
                on_delete: Cascade,
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "connections_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen with connections failed");

    // Schema::connections should contain SchemaConnection
    assert!(
        generated.contains("SchemaConnection"),
        "Expected SchemaConnection in generated metadata"
    );
    assert!(
        generated.contains("owner"),
        "Expected connection name 'owner' in generated code"
    );
    assert!(
        generated.contains("from_table"),
        "Expected from_table in SchemaConnection"
    );
    assert!(
        generated.contains("cardinality"),
        "Expected cardinality in SchemaConnection"
    );
    assert!(
        generated.contains("on_delete"),
        "Expected on_delete in SchemaConnection"
    );
}

#[test]
fn test_connections_generator_output() {
    // Schema with Record field + explicit connections (has model path and FK field)
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    UserCounter {
        table: "user_counter",
        version: "0.1.0",
        description: "Per-user counter",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::Record("user"), required: true },
            value: { r#type: FieldType::Integer, required: true, default: 0 },
        ],
        connections: [
            user: {
                table: "user",
                cardinality: HasOne,
                required: true,
                on_delete: Cascade,
                model: "crate::generated::User",
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "user_counter_connections_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen with connections failed");

    assert!(generated.contains("get_user"), "Expected get_user method");
    assert!(
        generated.contains("get_from_user"),
        "Expected get_from_user method"
    );
    assert!(
        generated.contains("get_from_user_id"),
        "Expected get_from_user_id method"
    );
    assert!(
        generated.contains("user_thing"),
        "Expected user_thing accessor"
    );
    assert!(
        generated.contains("where_user_has_results"),
        "Expected where_user_has_results method"
    );
    assert!(
        generated.contains("crate::generated::User"),
        "Expected crate::generated::User in generated code"
    );
    // Hop query method
    assert!(
        generated.contains("query_user"),
        "Expected query_user hop method for HasOne connection"
    );
    assert!(
        generated.contains("crate::generated::UserQuery"),
        "Expected target query type crate::generated::UserQuery in hop method"
    );
    assert!(
        generated.contains("HopType::HasOneForward"),
        "Expected HopType::HasOneForward in hop method"
    );
}

#[test]
fn test_hop_query_hasmany_generated() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestUser {
        table: "test_user",
        version: "0.1.0",
        description: "User with HasMany posts",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
        connections: [
            posts: {
                table: "test_post",
                cardinality: HasMany,
                reverse_field: "author",
                on_delete: Cascade,
                model: "crate::generated::TestPost",
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "hasmany_hop_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen with HasMany connection failed");

    assert!(
        generated.contains("query_posts"),
        "Expected query_posts hop method for HasMany connection"
    );
    assert!(
        generated.contains("crate::generated::TestPostQuery"),
        "Expected target query type crate::generated::TestPostQuery in hop method"
    );
    assert!(
        generated.contains("HopType::HasManyForward"),
        "Expected HopType::HasManyForward in hop method"
    );
    assert!(
        generated.contains("where_posts_has_results"),
        "Expected where_posts_has_results filter method"
    );
}

#[test]
fn test_hop_query_manytomany_generated() {
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestArticle {
        table: "test_article",
        version: "0.1.0",
        description: "Article with ManyToMany tags",

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
        ],
        connections: [
            tags: {
                table: "test_tag",
                cardinality: ManyToMany,
                edge_table: "article_tag",
                on_delete: Cascade,
                model: "crate::generated::TestTag",
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "manytomany_hop_schema.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen with ManyToMany connection failed");

    assert!(
        generated.contains("query_tags"),
        "Expected query_tags hop method for ManyToMany connection"
    );
    assert!(
        generated.contains("crate::generated::TestTagQuery"),
        "Expected target query type crate::generated::TestTagQuery in hop method"
    );
    assert!(
        generated.contains("HopType::ManyToManyForward"),
        "Expected HopType::ManyToManyForward in hop method"
    );
    assert!(
        generated.contains("where_tags_has_results"),
        "Expected where_tags_has_results filter method"
    );
}

#[test]
fn test_no_policies_on_connection() {
    // Connections without policies key should parse (policies omitted per design)
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Task {
        table: "task",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
            assignee: { r#type: FieldType::Record("user"), required: false },
        ],
        connections: [
            assignee: {
                table: "user",
                cardinality: HasOne,
                required: false,
                on_delete: SetNull,
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "no_policies_connection_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_ok(),
        "Connections without policies should parse: {:?}",
        result.err()
    );
}

#[test]
fn test_record_field_with_inferred_connection_passes() {
    // No explicit connections block - inferred from Record fields
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    InferredConn {
        table: "inferred_conn",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::Record("user"), required: true },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "inferred_connection_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_ok(),
        "Record field with inferred connection should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_hasmany_connection_without_local_fk_passes() {
    // HasMany: FK is on the other table. This table has no FK - connection only.
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    User {
        table: "user",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
        connections: [
            posts: {
                table: "post",
                cardinality: HasMany,
                reverse_field: "user",
                on_delete: Cascade,
                model: "crate::Post",
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "hasmany_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_ok(),
        "HasMany connection without local FK should parse (codegen may not generate get_posts yet): {:?}",
        result.err()
    );
    // Note: HasMany get_posts() is not yet generated - this test documents that
    // the schema parses and validation allows HasMany without local FK.
}

#[test]
fn test_manytomany_connection_without_fk_passes() {
    // ManyToMany: uses RELATE, no FK on either table
    let schema = r#"
use valence::prelude::*;

valence_schema! {
    Article {
        table: "article",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
        ],
        connections: [
            tags: {
                table: "tag",
                cardinality: ManyToMany,
                edge_table: "article_tag",
                on_delete: Cascade,
                model: "crate::Tag",
            },
        ],
    }
}
"#;

    let path = write_temp_schema_file(schema, "manytomany_schema.rs");
    let result = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new());
    assert!(
        result.is_ok(),
        "ManyToMany connection without FK should parse: {:?}",
        result.err()
    );
}
