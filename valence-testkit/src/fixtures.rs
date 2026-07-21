//! Shared fixtures for scenarios and bench.

use std::sync::OnceLock;

use serde_json::{json, Value};
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy::PrivacyRule;
use valence_core::privacy_policies::common;
use valence_core::schema::SchemaMetadata;
use valence_core::schema_api::{
    Schema, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules, SchemaPrivacy,
};
use valence_core::DatabaseEvaluator;

/// Opaque actor JSON for factory/bootstrap scenarios.
pub fn smoke_actor_json() -> Value {
    json!({"role": "system", "subject": "valence-testkit"})
}

/// Schema requiring authentication for read (catalog privacy sad-path).
pub fn authenticated_only_schema() -> &'static SchemaMetadata {
    static AUTH_EVAL: PrivacyRule = common::AUTHENTICATED;
    static METADATA: OnceLock<SchemaMetadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        let schema = Box::leak(Box::new(Schema {
            name: "catalog_auth_only".to_string(),
            version: "0.1.0".to_string(),
            databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "authenticated".to_string(),
                write: "authenticated".to_string(),
            },
            policies: Some(SchemaPolicies {
                read: Some(SchemaPolicyRules {
                    allow: vec![SchemaPolicyRule {
                        name: "AUTHENTICATED".to_string(),
                        description: None,
                        evaluator: Some(&AUTH_EVAL),
                    }],
                    ..SchemaPolicyRules::default()
                }),
                create: Some(SchemaPolicyRules {
                    allow: vec![SchemaPolicyRule {
                        name: "AUTHENTICATED".to_string(),
                        description: None,
                        evaluator: Some(&AUTH_EVAL),
                    }],
                    ..SchemaPolicyRules::default()
                }),
                ..SchemaPolicies::default()
            }),
            fields: vec![],
            edges: Vec::new(),
            connections: Vec::new(),
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            ttl: None,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: None,
            },
        }));
        SchemaMetadata::from_schema(schema)
    })
}

/// Invalid router compound key for the given storage slug (catalog sad-path).
///
/// Uses the storage adapter's own engine id so the sad path proves an
/// unregistered logical name fails under the engine actually being tested.
pub fn invalid_router_key(storage_slug: &str) -> String {
    use valence_core::KnownEngines;
    let engine_id = match storage_slug {
        "sqlite" => KnownEngines::SQLITE,
        "postgres" => KnownEngines::POSTGRES,
        "mongodb" => KnownEngines::MONGODB,
        "indradb" => KnownEngines::INDRADB,
        "hybrid" => KnownEngines::HYBRID_INDRA_SQL,
        "redis" => KnownEngines::REDIS,
        "surreal-mem" | "surreal-rocksdb" => KnownEngines::SURREALDB,
        "acme-stub" => "acme_stub",
        _ => KnownEngines::INMEMORY_MEM,
    };
    valence_core::router_key("nonexistent_logical", engine_id)
}
