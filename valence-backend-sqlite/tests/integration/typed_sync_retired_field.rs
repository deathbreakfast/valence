//! Boot sync keeps a retired column in place and leaves it out of reads.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, OnceLock};

use sqlx::Row;
use valence_backend_sql::sync_typed_table_sqlite;
use valence_backend_sqlite::SqliteBackend;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::schema::{SchemaMetadata, SchemaMetadataInit};
use valence_core::schema_api::{Schema, SchemaField, SchemaMeta, SchemaPrivacy};
use valence_core::storage_layout::{FieldStorage, LayoutField, StorageLayout};
use valence_core::{CompiledQuery, DatabaseBackend};

fn id_field() -> LayoutField {
    LayoutField {
        name: "id".into(),
        storage: FieldStorage::String,
        primary_key: true,
        nullable: false,
        unique: true,
        indexed: false,
        default: None,
        record_table: None,
    }
}

fn string_field(name: &str, nullable: bool) -> LayoutField {
    LayoutField {
        name: name.into(),
        storage: FieldStorage::String,
        primary_key: false,
        nullable,
        unique: false,
        indexed: false,
        default: None,
        record_table: None,
    }
}

/// `a, b` table as it existed before `b` was retired.
fn layout_v1(table: &str, b_nullable: bool) -> StorageLayout {
    StorageLayout {
        table: table.into(),
        fields: vec![
            id_field(),
            string_field("a", true),
            string_field("b", b_nullable),
        ],
        retired: Vec::new(),
    }
}

/// Same table after the schema dropped `b`, optionally listing it as retired.
fn layout_v2(table: &str, retired: &[&str]) -> StorageLayout {
    StorageLayout {
        table: table.into(),
        fields: vec![id_field(), string_field("a", true)],
        retired: retired.iter().map(|r| (*r).to_string()).collect(),
    }
}

fn schema_field(name: &str, primary: bool) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        field_type: "string".to_string(),
        primary,
        nullable: !primary,
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

fn leak_retired_schema(table: &str) -> &'static Schema {
    Box::leak(Box::new(Schema {
        name: table.to_string(),
        version: "1.1.0".to_string(),
        databases: vec!["default".to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "public".to_string(),
            write: "service".to_string(),
        },
        policies: None,
        fields: vec![schema_field("id", true), schema_field("a", false)],
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        retired_fields: vec!["b".to_string()],
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
    }))
}

const READ_TABLE: &str = "retired_read_demo";

/// Submitted through inventory, so the merged test binary's global registry
/// holds this fixture whichever test reads the registry first.
fn read_table_meta() -> &'static SchemaMetadata {
    static META: OnceLock<&'static SchemaMetadata> = OnceLock::new();
    META.get_or_init(|| {
        Box::leak(Box::new(SchemaMetadata::from_schema(leak_retired_schema(
            READ_TABLE,
        ))))
    })
}

valence_core::inventory::submit! { SchemaMetadataInit(read_table_meta) }

async fn column_names(backend: &SqliteBackend, table: &str) -> Vec<String> {
    sqlx::query(&format!("PRAGMA table_info(\"{table}\")"))
        .fetch_all(backend.pool())
        .await
        .expect("pragma")
        .iter()
        .map(|r| r.get::<String, _>("name"))
        .collect()
}

#[tokio::test]
async fn retired_field_sync_sqlite_accepts_orphan_happy() {
    let table = "retired_sync_demo";
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    backend
        .ensure_typed_table(&layout_v1(table, true))
        .await
        .expect("ensure v1");

    sync_typed_table_sqlite(backend.pool(), &layout_v2(table, &["b"]))
        .await
        .expect("sync accepts retired b");
    assert!(column_names(&backend, table)
        .await
        .contains(&"b".to_string()));

    backend
        .create_record(table, serde_json::json!({"id": "r1", "a": "alpha"}))
        .await
        .expect("create");
    let b: Option<String> = sqlx::query(&format!("SELECT b FROM \"{table}\" WHERE id = 'r1'"))
        .fetch_one(backend.pool())
        .await
        .expect("select b")
        .get("b");
    assert_eq!(b, None);
}

#[tokio::test]
async fn unlisted_orphan_still_refused_sad() {
    let table = "retired_unlisted_demo";
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    backend
        .ensure_typed_table(&layout_v1(table, true))
        .await
        .expect("ensure v1");

    let err = sync_typed_table_sqlite(backend.pool(), &layout_v2(table, &[]))
        .await
        .expect_err("unlisted orphan b must be refused");
    let msg = err.to_string();
    assert!(msg.contains("refuses drop of live field"), "{msg}");
    assert!(msg.contains(&format!("{table}.b")), "{msg}");
}

#[tokio::test]
async fn retired_not_null_column_refused_sad() {
    let table = "retired_not_null_demo";
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    backend
        .ensure_typed_table(&layout_v1(table, false))
        .await
        .expect("ensure v1");

    let err = sync_typed_table_sqlite(backend.pool(), &layout_v2(table, &["b"]))
        .await
        .expect_err("NOT NULL retired b without DEFAULT must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains(&format!("retired NOT NULL column {table}.b")),
        "{msg}"
    );
}

#[tokio::test]
async fn retired_field_not_in_reads_happy() {
    let table = READ_TABLE;

    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    backend
        .ensure_typed_table(&layout_v1(table, true))
        .await
        .expect("ensure v1");
    // A row written while `b` was still a live field.
    sqlx::query(&format!(
        "INSERT INTO \"{table}\" (id, a, b) VALUES ('old', 'alpha', 'stale')"
    ))
    .execute(backend.pool())
    .await
    .expect("seed old row");

    let layout = StorageLayout::from_schema(leak_retired_schema(table)).expect("layout");
    assert_eq!(layout.retired, vec!["b".to_string()]);
    sync_typed_table_sqlite(backend.pool(), &layout)
        .await
        .expect("sync");

    // `get_record` is `get_record_typed_sqlite`.
    let got = backend
        .get_record(table, "old")
        .await
        .expect("get")
        .expect("row present");
    assert_eq!(got["a"], "alpha");
    assert!(got.get("b").is_none(), "get leaked retired b: {got}");

    let rows = backend
        .execute_compiled_query(&CompiledQuery::new(
            format!("SELECT * FROM {table}"),
            Vec::new(),
        ))
        .await
        .expect("query");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["a"], "alpha");
    assert!(
        rows[0].get("b").is_none(),
        "query leaked retired b: {rows:?}"
    );
}
