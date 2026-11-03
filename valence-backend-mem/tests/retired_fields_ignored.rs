//! A schema with `retired_fields` registers and works on the mem backend.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{Schema, SchemaField, SchemaMeta, SchemaPrivacy};
use valence_core::{DatabaseBackend, Valence};

fn leak_retired_schema(table: &str) -> &'static Schema {
    Box::leak(Box::new(Schema {
        name: table.to_string(),
        version: "1.0.0".to_string(),
        databases: vec!["default".to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "public".to_string(),
            write: "service".to_string(),
        },
        policies: None,
        fields: vec![SchemaField {
            name: "id".to_string(),
            field_type: "string".to_string(),
            primary: true,
            nullable: false,
            indexed: false,
            unique: false,
            default: None,
            fk: None,
            validations: Vec::new(),
            policies: None,
            encrypted: false,
            enum_variants: Vec::new(),
            enum_type: None,
            model_path: None,
        }],
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        retired_fields: vec!["legacy".to_string()],
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,

            repository: "https://github.com/unified-field-dev/valence".to_string(),
        },
    }))
}

#[tokio::test]
async fn retired_fields_mem_backend_ignored_happy() {
    let table = "retired_mem_probe";
    let schema = leak_retired_schema(table);
    let mut registry = SchemaRegistry::new();
    registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(schema))));
    SchemaRegistry::set_global(registry);
    assert_eq!(
        SchemaRegistry::global()
            .get_full_schema(table)
            .expect("registered")
            .retired_fields,
        vec!["legacy".to_string()]
    );

    let backend = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .unwrap();
    valence.sync_typed_tables_from_registry().await.unwrap();

    backend
        .create_record(table, serde_json::json!({"id": "m1"}))
        .await
        .unwrap();
    let got = backend.get_record(table, "m1").await.unwrap();
    assert!(got.is_some(), "row readable after create");
}
