//! Deferred TTL stamp on create (registry fixture + mem backend).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{Schema, SchemaField, SchemaMeta, SchemaPrivacy};
use valence_core::ttl::{SchemaTtlPolicy, EXPIRE_AT_FIELD};
use valence_core::{DatabaseBackend, Valence};

fn leak_ttl_schema(table: &str, seconds: u64) -> &'static Schema {
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
        ttl: Some(SchemaTtlPolicy {
            seconds,
            mode: "backend_capability".into(),
        }),
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,
        },
    }))
}

#[tokio::test]
async fn mem_create_stamps_expire_at_and_ensure_ok() {
    let table = "ttl_mem_deferred_probe";
    let mut registry = SchemaRegistry::new();
    registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(
        leak_ttl_schema(table, 1800),
    ))));
    SchemaRegistry::set_global(registry);

    let backend = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .unwrap();

    valence.ensure_ttl_for_all().await.unwrap();
    valence.ensure_ttl_for_table(table).await.unwrap();

    let created = backend
        .create_record(table, serde_json::json!({"id": "a1", "n": 1}))
        .await
        .unwrap();
    let expire = created
        .get(EXPIRE_AT_FIELD)
        .and_then(|v| v.as_str())
        .expect("__valence_expire_at on create")
        .to_string();

    // Merge does not call prepare_create_content; expire stamp must remain create-only.
    let merged = backend
        .merge_record(table, "a1", serde_json::json!({"n": 2}))
        .await
        .unwrap();
    assert_eq!(
        merged.get(EXPIRE_AT_FIELD).and_then(|v| v.as_str()),
        Some(expire.as_str()),
        "merge must not refresh create-only expire"
    );
}
