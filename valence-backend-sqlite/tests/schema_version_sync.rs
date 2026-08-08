//! Version-gated boot sync: stamp skip + bump add-field.

use std::sync::Arc;

use valence_backend_sqlite::SqliteBackend;
use valence_core::storage_layout::{
    sync_typed_table_for, FieldStorage, LayoutField, StorageLayout,
};
use valence_core::{DatabaseBackend, SchemaRegistry, Valence};

#[tokio::test]
async fn sqlite_version_stamp_skips_second_sync() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let layout = StorageLayout {
        table: "ver_stamp_demo".into(),
        fields: vec![
            LayoutField {
                name: "id".into(),
                storage: FieldStorage::String,
                primary_key: true,
                nullable: false,
                unique: true,
                indexed: false,
                default: None,
            },
            LayoutField {
                name: "name".into(),
                storage: FieldStorage::String,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
                default: None,
            },
        ],
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
        fields: vec![
            LayoutField {
                name: "id".into(),
                storage: FieldStorage::String,
                primary_key: true,
                nullable: false,
                unique: true,
                indexed: false,
                default: None,
            },
            LayoutField {
                name: "name".into(),
                storage: FieldStorage::String,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
                default: None,
            },
        ],
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
