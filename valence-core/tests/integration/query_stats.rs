//! `QueryCore::execute_with_stats` reports backend rows before read privacy.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, OnceLock};

use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy_policies::common::AUTHENTICATED;
use valence_core::privacy_policies::owner::OWNER_BY_USER_FIELD;
use valence_core::query::QueryCore;
use valence_core::schema::{SchemaMetadata, SchemaMetadataInit};
use valence_core::schema_api::{
    Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
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
        partition_export: None,
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

const TABLES: [&str; 6] = [
    "qs_hidden",
    "qs_end",
    "qs_all_hidden",
    "qs_same",
    "qs_page",
    "qs_budget",
];

/// Each fixture table, submitted through inventory so the merged test binary's
/// global registry holds them whichever test reads the registry first.
fn table_meta<const I: usize>() -> &'static SchemaMetadata {
    static METAS: [OnceLock<&'static SchemaMetadata>; TABLES.len()] =
        [const { OnceLock::new() }; TABLES.len()];
    METAS[I].get_or_init(|| owner_schema(TABLES[I]))
}

valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<0>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<1>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<2>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<3>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<4>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<5>) }

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Runs windowed queries so the suite can compare returned rows with the backend row count. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

/// Ten rows `r00..r09`; alice owns `r01 r03 r04 r08`, bob the rest.
async fn seed(table: &str) -> Valence {
    assert!(TABLES.contains(&table));
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
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
    // Skipping two readable rows leaves r04 and r08; the table ends before the page fills.
    let (rows, fetched) = run(&v, "qs_end", 2, 10).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(fetched, 10);
}

#[tokio::test]
async fn query_stats_all_hidden_window_sad() {
    let v = seed("qs_all_hidden").await;
    // Alice owns four rows, so offset 4 skips every readable row.
    let (rows, fetched) = run(&v, "qs_all_hidden", 4, 3).await;
    assert_eq!(rows.len(), 0);
    assert_eq!(fetched, 10);
}

#[tokio::test]
async fn page_fills_past_hidden_rows_happy() {
    let v = seed("qs_page").await;
    let (rows, _) = run(&v, "qs_page", 0, 2).await;
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["r01", "r03"]);
    // OFFSET counts readable rows, not backend rows.
    let (rows, fetched) = run(&v, "qs_page", 1, 2).await;
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["r03", "r04"]);
    assert_eq!(fetched, 6);
}

#[tokio::test]
async fn limit_above_readable_rows_returns_all_readable_sad() {
    let v = seed("qs_budget").await;
    // Only four rows are readable; a limit of 100 returns them and stops at table end.
    let (rows, fetched) = run(&v, "qs_budget", 0, 100).await;
    assert_eq!(rows.len(), 4);
    assert_eq!(fetched, 10);
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
