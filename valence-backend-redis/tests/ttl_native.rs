//! Env-gated Redis native TTL (create EXPIRE; update does not refresh).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::sync::Arc;

use redis::AsyncCommands;
use valence_backend_redis::{RedisBackend, KEY_PREFIX_ENV, TEST_URL_ENV, URL_ENV};
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{Schema, SchemaField, SchemaMeta, SchemaPrivacy};
use valence_core::ttl::SchemaTtlPolicy;
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
async fn redis_ttl_create_sets_expire_not_refreshed_on_update() {
    let backend = match RedisBackend::builder().from_env_defaults().build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("redis unavailable: {e} — skipping ttl native test");
            return;
        }
    };

    let table = "ttl_redis_native_probe";
    let id = "r1";
    let mut registry = SchemaRegistry::new();
    registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(
        leak_ttl_schema(table, 120),
    ))));
    SchemaRegistry::set_global(registry);

    let backend = Arc::new(backend);
    assert_eq!(
        backend.ttl_capability(),
        valence_core::ttl::BackendTtlCapability::SupportedNative
    );

    backend
        .create_record(table, serde_json::json!({"id": id, "n": 1}))
        .await
        .unwrap();

    let url = if let Ok(u) = std::env::var(TEST_URL_ENV).or_else(|_| std::env::var(URL_ENV)) {
        u
    } else {
        eprintln!("no redis URL env — skipping TTL key check");
        return;
    };
    let prefix = std::env::var(KEY_PREFIX_ENV).unwrap_or_else(|_| "valence".into());
    let doc_key = format!("{prefix}:doc:{table}:{id}");
    let client = redis::Client::open(url.as_str()).unwrap();
    let mut conn = redis::aio::ConnectionManager::new(client).await.unwrap();
    let ttl_before: i64 = conn.ttl(&doc_key).await.unwrap();
    assert!(
        ttl_before > 0 && ttl_before <= 120,
        "expected EXPIRE on create, got ttl={ttl_before} for {doc_key}"
    );

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    backend
        .update_record(table, id, serde_json::json!({"id": id, "n": 2}))
        .await
        .unwrap();
    let ttl_after: i64 = conn.ttl(&doc_key).await.unwrap();
    assert!(
        ttl_after > 0 && ttl_after <= ttl_before,
        "update must not refresh Redis TTL (before={ttl_before}, after={ttl_after})"
    );
}
