//! Env-gated Mongo native TTL index + create stamp.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::sync::{Arc, OnceLock};

use valence_backend_mongodb::MongoBackend;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::schema::{SchemaMetadata, SchemaMetadataInit, SchemaRegistry};
use valence_core::schema_api::{Schema, SchemaField, SchemaMeta, SchemaPrivacy};
use valence_core::ttl::{SchemaTtlPolicy, EXPIRE_AT_FIELD};
use valence_core::DatabaseBackend;

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
        retired_fields: Vec::new(),
        ttl: Some(SchemaTtlPolicy {
            seconds,
            mode: "backend_capability".into(),
        }),
        partition_export: None,
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

/// Submitted through inventory, so the merged test binary's global registry
/// holds this fixture whichever test reads the registry first.
fn ttl_probe_meta() -> &'static SchemaMetadata {
    static META: OnceLock<&'static SchemaMetadata> = OnceLock::new();
    META.get_or_init(|| {
        Box::leak(Box::new(SchemaMetadata::from_schema(leak_ttl_schema(
            "ttl_mongo_native_probe",
            1800,
        ))))
    })
}

valence_core::inventory::submit! { SchemaMetadataInit(ttl_probe_meta) }

#[tokio::test]
async fn mongo_apply_ttl_policy_and_create_stamps() {
    let backend = match MongoBackend::builder().from_env_defaults().build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("mongodb unavailable: {e} — skipping ttl native test");
            return;
        }
    };

    let table = "ttl_mongo_native_probe";

    let backend = Arc::new(backend);
    assert_eq!(
        backend.ttl_capability(),
        valence_core::ttl::BackendTtlCapability::SupportedNative
    );
    let policy = SchemaTtlPolicy {
        seconds: 1800,
        mode: "backend_capability".into(),
    };
    backend.apply_ttl_policy(table, &policy).await.unwrap();
    // Idempotent second call
    backend.apply_ttl_policy(table, &policy).await.unwrap();

    let created = backend
        .create_record(table, serde_json::json!({"id": "m1", "n": 1}))
        .await
        .unwrap();
    assert!(
        created.get(EXPIRE_AT_FIELD).is_some(),
        "mongo create must stamp {EXPIRE_AT_FIELD}"
    );

    let first = created[EXPIRE_AT_FIELD].clone();
    let updated = backend
        .update_record(
            table,
            "m1",
            serde_json::json!({"id": "m1", "n": 2, EXPIRE_AT_FIELD: first}),
        )
        .await
        .unwrap();
    assert_eq!(updated.get(EXPIRE_AT_FIELD), created.get(EXPIRE_AT_FIELD));
}
