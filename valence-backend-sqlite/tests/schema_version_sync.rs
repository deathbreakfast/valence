//! Version-gated boot sync: stamp skip + bump add-field + sad paths.

use std::sync::Arc;

use valence_backend_sqlite::SqliteBackend;
use valence_core::error::Error;
use valence_core::storage_layout::{
    sync_typed_table_for, FieldStorage, LayoutField, StorageLayout,
};
use valence_core::{DatabaseBackend, SchemaRegistry, Valence};

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

#[tokio::test]
async fn sqlite_version_stamp_skips_second_sync() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let layout = StorageLayout {
        table: "ver_stamp_demo".into(),
        fields: vec![id_field(), string_field("name", true)],
    };
    backend.ensure_typed_table(&layout).await.expect("ensure");
    backend
        .write_schema_version("ver_stamp_demo", "1.0.0")
        .await
        .expect("stamp");
    let again = backend
        .read_schema_version("ver_stamp_demo")
        .await
        .expect("read");
    assert_eq!(again.as_deref(), Some("1.0.0"));

    // Second sync with same layout is a no-op at the backend layer; stamp stays.
    backend.sync_typed_table(&layout).await.expect("sync");
    assert_eq!(
        backend
            .read_schema_version("ver_stamp_demo")
            .await
            .unwrap()
            .as_deref(),
        Some("1.0.0")
    );
}

#[tokio::test]
async fn sqlite_bump_adds_column_via_sync() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let layout_v1 = StorageLayout {
        table: "ver_bump_demo".into(),
        fields: vec![id_field(), string_field("name", true)],
    };
    backend
        .ensure_typed_table(&layout_v1)
        .await
        .expect("ensure");
    backend
        .create_record(
            "ver_bump_demo",
            serde_json::json!({"id": {"table":"ver_bump_demo","id":"r1"}, "name": "a"}),
        )
        .await
        .expect("create");

    let mut layout_v2 = layout_v1.clone();
    layout_v2.fields.push(LayoutField {
        name: "score".into(),
        storage: FieldStorage::Integer,
        primary_key: false,
        nullable: true,
        unique: false,
        indexed: false,
        default: None,
    record_table: None,
    });
    backend
        .sync_typed_table(&layout_v2)
        .await
        .expect("sync bump");
    backend
        .write_schema_version("ver_bump_demo", "1.1.0")
        .await
        .expect("stamp");

    let inspected = backend
        .inspect_typed_layout("ver_bump_demo")
        .await
        .expect("inspect")
        .expect("present");
    assert!(inspected.fields.iter().any(|f| f.name == "score"));
    assert_eq!(
        backend
            .read_schema_version("ver_bump_demo")
            .await
            .unwrap()
            .as_deref(),
        Some("1.1.0")
    );

    let _ = Valence::builder()
        .add_backend("default", backend)
        .build()
        .expect("build");
}

#[tokio::test]
async fn registry_stamp_match_skips_resync() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .expect("build");
    valence
        .sync_typed_tables_from_registry()
        .await
        .expect("boot sync");

    let table = valence_testkit::CATALOG_TTL_PROBE_TABLE;
    let meta = SchemaRegistry::global()
        .get_schema(table)
        .expect("ttl probe in registry");
    let stamp = backend
        .read_schema_version(table)
        .await
        .expect("read")
        .expect("stamped");
    assert_eq!(stamp, meta.version);

    sync_typed_table_for(&valence, table)
        .await
        .expect("second sync skip");
    assert_eq!(
        backend.read_schema_version(table).await.unwrap().as_deref(),
        Some(meta.version)
    );
}

#[tokio::test]
async fn registry_forced_stamp_mismatch_resyncs() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .expect("build");
    valence
        .sync_typed_tables_from_registry()
        .await
        .expect("boot sync");

    let table = valence_testkit::CATALOG_TTL_PROBE_TABLE;
    let meta = SchemaRegistry::global()
        .get_schema(table)
        .expect("ttl probe in registry");
    backend
        .write_schema_version(table, "0.0.0-force")
        .await
        .expect("force mismatch");
    sync_typed_table_for(&valence, table).await.expect("resync");
    assert_eq!(
        backend.read_schema_version(table).await.unwrap().as_deref(),
        Some(meta.version)
    );
}

/// Stale stamp + incomplete table → `sync_typed_table_for` ADDs registry columns and restamps.
#[tokio::test]
async fn registry_version_gate_adds_missing_column() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .expect("build");

    let table = valence_testkit::CATALOG_TTL_PROBE_TABLE;
    let meta = SchemaRegistry::global()
        .get_schema(table)
        .expect("ttl probe in registry");

    // Physical table behind the registry layout (id only).
    backend
        .ensure_typed_table(&StorageLayout {
            table: table.into(),
            fields: vec![id_field()],
        })
        .await
        .expect("id-only ensure");
    backend
        .write_schema_version(table, "0.0.0-stale")
        .await
        .expect("stale stamp");

    sync_typed_table_for(&valence, table)
        .await
        .expect("version-gate sync");

    let inspected = backend
        .inspect_typed_layout(table)
        .await
        .expect("inspect")
        .expect("present");
    assert!(
        inspected.fields.iter().any(|f| f.name == "n"),
        "expected registry field n after gated sync: {inspected:?}"
    );
    assert_eq!(
        backend.read_schema_version(table).await.unwrap().as_deref(),
        Some(meta.version)
    );
}

#[tokio::test]
async fn sqlite_nullability_change_refused_on_sync() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let live = StorageLayout {
        table: "ver_null_refuse".into(),
        fields: vec![id_field(), string_field("name", false)],
    };
    backend.ensure_typed_table(&live).await.expect("ensure");

    let desired = StorageLayout {
        table: "ver_null_refuse".into(),
        fields: vec![id_field(), string_field("name", true)],
    };
    let err = backend
        .sync_typed_table(&desired)
        .await
        .expect_err("nullability change must fail on sqlite");
    match err {
        Error::Validation(msg) => assert!(
            msg.contains("nullability"),
            "unexpected validation message: {msg}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

/// Stamp matches registry version while physical layout is missing a schema column → write refuses.
#[tokio::test]
async fn registry_write_missing_column_refuses() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let _valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .expect("build");

    let table = valence_testkit::CATALOG_TTL_PROBE_TABLE;
    let meta = SchemaRegistry::global()
        .get_schema(table)
        .expect("ttl probe in registry");

    backend
        .ensure_typed_table(&StorageLayout {
            table: table.into(),
            fields: vec![id_field()],
        })
        .await
        .expect("id-only");
    // Equal stamp ⇒ boot sync would skip; write must not silent-ADD.
    backend
        .write_schema_version(table, meta.version)
        .await
        .expect("stamp current");

    let err = backend
        .create_record(
            table,
            serde_json::json!({
                "id": {"table": table, "id": "missing_col_row"},
                "n": 1
            }),
        )
        .await
        .expect_err("missing column must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("missing column") && msg.contains(table),
        "unexpected error: {msg}"
    );
}
