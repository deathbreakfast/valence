//! Shared fixtures for scenarios and bench.

use std::sync::OnceLock;

use serde_json::Value;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy::PrivacyRule;
use valence_core::privacy_policies::common;
use valence_core::schema::SchemaMetadata;
use valence_core::schema_api::{
    Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
use valence_core::DatabaseEvaluator;

/// Typed [`valence_core::actor::Actor`] JSON for factory/bootstrap scenarios.
pub fn smoke_actor_json() -> Value {
    serde_json::to_value(valence_core::actor::Actor::System {
        operation: "valence-testkit".into(),
    })
    .unwrap_or(Value::Null)
}

/// Schema with no entity policies (catalog default-deny sad-path).
pub fn empty_policies_schema() -> &'static SchemaMetadata {
    static METADATA: OnceLock<SchemaMetadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        let schema = Box::leak(Box::new(Schema {
            name: "catalog_empty_policies".to_string(),
            version: "0.1.0".to_string(),
            databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "none".to_string(),
                write: "none".to_string(),
            },
            policies: None,
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

/// Schema with a `SYSTEM_ONLY` field for field-privacy sad paths.
pub fn system_only_field_schema() -> &'static SchemaMetadata {
    static PUBLIC: PrivacyRule = common::PUBLIC_READ;
    static SYSTEM: PrivacyRule = common::SYSTEM_ONLY;
    static METADATA: OnceLock<SchemaMetadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        let schema = Box::leak(Box::new(Schema {
            name: "catalog_field_privacy".to_string(),
            version: "0.1.0".to_string(),
            databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "public".to_string(),
            },
            policies: Some(SchemaPolicies {
                read: Some(SchemaPolicyRules {
                    allow: vec![SchemaPolicyRule {
                        name: "PUBLIC_READ".to_string(),
                        description: None,
                        evaluator: Some(&PUBLIC),
                    }],
                    ..SchemaPolicyRules::default()
                }),
                ..SchemaPolicies::default()
            }),
            fields: vec![
                SchemaField {
                    name: "id".to_string(),
                    field_type: "string".to_string(),
                    primary: true,
                    nullable: false,
                    indexed: false,
                    unique: false,
                    default: None,
                    fk: None,
                    validations: Vec::new(),
                    policies: Some(SchemaPolicies {
                        read: Some(SchemaPolicyRules {
                            allow: vec![SchemaPolicyRule {
                                name: "PUBLIC_READ".to_string(),
                                description: None,
                                evaluator: Some(&PUBLIC),
                            }],
                            ..SchemaPolicyRules::default()
                        }),
                        ..SchemaPolicies::default()
                    }),
                    encrypted: false,
                    enum_variants: Vec::new(),
                    enum_type: None,
                    model_path: None,
                },
                SchemaField {
                    name: "secret".to_string(),
                    field_type: "string".to_string(),
                    primary: false,
                    nullable: true,
                    indexed: false,
                    unique: false,
                    default: None,
                    fk: None,
                    validations: Vec::new(),
                    policies: Some(SchemaPolicies {
                        read: Some(SchemaPolicyRules {
                            allow: vec![SchemaPolicyRule {
                                name: "SYSTEM_ONLY".to_string(),
                                description: None,
                                evaluator: Some(&SYSTEM),
                            }],
                            ..SchemaPolicyRules::default()
                        }),
                        ..SchemaPolicies::default()
                    }),
                    encrypted: false,
                    enum_variants: Vec::new(),
                    enum_type: None,
                    model_path: None,
                },
            ],
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

/// Catalog TTL probe table name (short create-only TTL for matrix e2e).
pub const CATALOG_TTL_PROBE_TABLE: &str = "catalog_ttl_probe";

/// TTL seconds on [`CATALOG_TTL_PROBE_TABLE`] (short enough for Redis expiry waits).
pub const CATALOG_TTL_PROBE_SECONDS: u64 = 2;

fn catalog_ttl_probe_schema() -> &'static Schema {
    static SCHEMA: OnceLock<&'static Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        Box::leak(Box::new(Schema {
            name: CATALOG_TTL_PROBE_TABLE.to_string(),
            version: "0.1.1".to_string(),
            databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "public".to_string(),
            },
            policies: None,
            fields: vec![
                SchemaField {
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
                },
                SchemaField {
                    name: "n".to_string(),
                    field_type: "integer".to_string(),
                    primary: false,
                    nullable: true,
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
                },
            ],
            edges: Vec::new(),
            connections: Vec::new(),
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            ttl: Some(valence_core::ttl::SchemaTtlPolicy {
                seconds: CATALOG_TTL_PROBE_SECONDS,
                mode: "backend_capability".into(),
            }),
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: Some("matrix TTL probe".into()),
            },
        }))
    })
}

/// Schema metadata for the TTL matrix probe (also submitted via inventory).
pub fn catalog_ttl_probe_metadata() -> &'static SchemaMetadata {
    static METADATA: OnceLock<SchemaMetadata> = OnceLock::new();
    METADATA.get_or_init(|| SchemaMetadata::from_schema(catalog_ttl_probe_schema()))
}

fn catalog_ttl_probe_inventory_init() -> &'static SchemaMetadata {
    catalog_ttl_probe_metadata()
}

valence_core::inventory::submit! {
    valence_core::SchemaMetadataInit(catalog_ttl_probe_inventory_init)
}

/// Catalog iter multi-page probe table (no TTL).
pub const CATALOG_ITER_PROBE_TABLE: &str = "catalog_iter_probe";

fn catalog_iter_probe_schema() -> &'static Schema {
    static SCHEMA: OnceLock<&'static Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        Box::leak(Box::new(Schema {
            name: CATALOG_ITER_PROBE_TABLE.to_string(),
            version: "0.1.1".to_string(),
            databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "public".to_string(),
            },
            policies: None,
            fields: vec![
                SchemaField {
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
                },
                SchemaField {
                    name: "n".to_string(),
                    field_type: "integer".to_string(),
                    primary: false,
                    nullable: true,
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
                },
            ],
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
                description: Some("matrix iter scan probe".into()),
            },
        }))
    })
}

fn catalog_iter_probe_inventory_init() -> &'static SchemaMetadata {
    static METADATA: OnceLock<SchemaMetadata> = OnceLock::new();
    METADATA.get_or_init(|| SchemaMetadata::from_schema(catalog_iter_probe_schema()))
}

valence_core::inventory::submit! {
    valence_core::SchemaMetadataInit(catalog_iter_probe_inventory_init)
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
