//! `QueryCore::execute_with_stats` on SQLite reports backend rows before read privacy.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_sqlite::SqliteBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy_policies::common::AUTHENTICATED;
use valence_core::privacy_policies::owner::OWNER_BY_USER_FIELD;
use valence_core::query::QueryCore;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
use valence_core::storage_layout::{FieldStorage, LayoutField, StorageLayout};
use valence_core::{DatabaseBackend, DatabaseEvaluator, Valence};

fn leak_schema(schema: Schema) -> &'static Schema {
    Box::leak(Box::new(schema))
}

fn base_schema(name: &str, policies: SchemaPolicies, fields: Vec<SchemaField>) -> &'static Schema {
    leak_schema(Schema {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".to_string(),
            write: "test".to_string(),
        },
        policies: Some(policies),
        fields,
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        retired_fields: Vec::new(),
        traits: Vec::new(),
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,

            repository: "https://github.com/unified-field-dev/valence".to_string(),
        },
    })
}

fn meta(schema: &'static Schema) -> &'static SchemaMetadata {
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

fn id_field() -> SchemaField {
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
    }
}

fn user_field() -> SchemaField {
    SchemaField {
        name: "user".to_string(),
        field_type: "string".to_string(),
        primary: false,
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
    }
}

fn allow_rule(name: &str, eval: &'static valence_core::privacy::PrivacyRule) -> SchemaPolicyRule {
    SchemaPolicyRule {
        name: name.to_string(),
        description: None,
        evaluator: Some(eval),
    }
}

fn owner_schema(table: &str) -> &'static SchemaMetadata {
    meta(base_schema(
        table,
        SchemaPolicies {
            read: Some(SchemaPolicyRules {
                allow: vec![allow_rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
            create: Some(SchemaPolicyRules {
                allow: vec![allow_rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        },
        vec![id_field(), user_field()],
    ))
}

const TABLES: [&str; 4] = ["qs_hidden", "qs_end", "qs_all_hidden", "qs_same"];

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Runs windowed queries so the suite can compare returned rows with the backend row count. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

/// Ten rows `r00..r09`; alice owns `r01 r03 r04 r08`, bob the rest.
async fn seed(table: &str) -> Valence {
    static INIT: std::sync::Once = std::sync::Once::new();
    // Queries resolve schemas through the global registry, which can be set once.
    INIT.call_once(|| {
        let mut registry = SchemaRegistry::new();
        for t in TABLES {
            registry.register(owner_schema(t));
        }
        SchemaRegistry::set_global(registry);
    });
    assert!(TABLES.contains(&table));
    let sqlite = SqliteBackend::connect_memory()
        .await
        .expect("connect sqlite");
    let col = |name: &str, primary_key: bool| LayoutField {
        name: name.into(),
        storage: FieldStorage::String,
        primary_key,
        nullable: !primary_key,
        unique: primary_key,
        indexed: false,
        default: None,
        record_table: None,
    };
    sqlite
        .ensure_typed_table(&StorageLayout {
            table: table.into(),
            fields: vec![col("id", true), col("user", false)],
            retired: Vec::new(),
        })
        .await
        .expect("ensure table");
    let backend: Arc<dyn DatabaseBackend> = Arc::new(sqlite);
    for i in 0..10 {
        let owner = if [1, 3, 4, 8].contains(&i) {
            "alice"
        } else {
            "bob"
        };
        backend
            .create_record(
                table,
                serde_json::json!({"id": format!("r{i:02}"), "user": owner}),
            )
            .await
            .unwrap();
    }
    Valence::builder()
        .add_backend("default", backend)
        .with_actor(Actor::User {
            user_id: "alice".into(),
        })
        .build()
        .expect("build")
}

async fn run(v: &Valence, table: &str, offset: u32, limit: u32) -> (Vec<serde_json::Value>, usize) {
    let (rows, stats) = QueryCore::new(table.to_string())
        .order_by("id".to_string(), valence_core::query::SortDirection::Asc)
        .offset(offset)
        .limit(limit)
        .execute_with_stats::<serde_json::Value>(v, purpose())
        .await
        .unwrap();
    (rows, stats.fetched)
}

#[tokio::test]
async fn query_stats_counts_hidden_rows_happy() {
    let v = seed("qs_hidden").await;
    let (rows, fetched) = run(&v, "qs_hidden", 0, 10).await;
    assert_eq!(rows.len(), 4);
    assert_eq!(fetched, 10);
}

#[tokio::test]
async fn query_stats_short_window_at_table_end_happy() {
    let v = seed("qs_end").await;
    let (rows, fetched) = run(&v, "qs_end", 8, 10).await;
    assert_eq!(rows.len(), 1, "only r08 is alice's in the last two rows");
    assert_eq!(fetched, 2);
}

#[tokio::test]
async fn query_stats_all_hidden_window_sad() {
    let v = seed("qs_all_hidden").await;
    // r05..r07 belong to bob.
    let (rows, fetched) = run(&v, "qs_all_hidden", 5, 3).await;
    assert_eq!(rows.len(), 0, "every row in the window belongs to bob");
    assert_eq!(fetched, 3);
}

#[tokio::test]
async fn query_stats_matches_execute_rows_happy() {
    let v = seed("qs_same").await;
    let plain: Vec<serde_json::Value> = QueryCore::new("qs_same".to_string())
        .order_by("id".to_string(), valence_core::query::SortDirection::Asc)
        .execute(&v, purpose())
        .await
        .unwrap();
    let (rows, fetched) = run(&v, "qs_same", 0, 100).await;
    assert_eq!(plain, rows);
    assert_eq!(fetched, 10);
}
