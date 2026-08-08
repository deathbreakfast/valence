//! Ensure + additive sync adds a column without dropping existing data.

use std::sync::Arc;

use valence_backend_sqlite::SqliteBackend;
use valence_core::storage_layout::{FieldStorage, LayoutField, StorageLayout};
use valence_core::{DatabaseBackend, Valence};

#[tokio::test]
async fn sqlite_sync_adds_column() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let layout_v1 = StorageLayout {
        table: "typed_sync_demo".into(),
        fields: vec![
            LayoutField {
                name: "id".into(),
                storage: FieldStorage::String,
                primary_key: true,
                nullable: false,
                unique: true,
                indexed: false,
            },
            LayoutField {
                name: "name".into(),
                storage: FieldStorage::String,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
            },
        ],
    };
    backend
        .ensure_typed_table(&layout_v1)
        .await
        .expect("ensure");
    let created = backend
        .create_record(
            "typed_sync_demo",
            serde_json::json!({"id": {"table":"typed_sync_demo","id":"r1"}, "name": "alpha"}),
        )
        .await
        .expect("create");
    assert_eq!(created["name"], "alpha");

    let mut layout_v2 = layout_v1.clone();
    layout_v2.fields.push(LayoutField {
        name: "score".into(),
        storage: FieldStorage::Integer,
        primary_key: false,
        nullable: true,
        unique: false,
        indexed: false,
    });
    backend.sync_typed_table(&layout_v2).await.expect("sync");

    let inspected = backend
        .inspect_typed_layout("typed_sync_demo")
        .await
        .expect("inspect")
        .expect("present");
    assert!(
        inspected.fields.iter().any(|f| f.name == "score"),
        "score column missing after sync: {inspected:?}"
    );

    backend
        .update_record(
            "typed_sync_demo",
            "r1",
            serde_json::json!({"name": "alpha", "score": 7}),
        )
        .await
        .expect("update");
    let got = backend
        .get_record("typed_sync_demo", "r1")
        .await
        .expect("get")
        .expect("row");
    assert_eq!(got["score"], 7);

    let valence = Valence::builder()
        .add_backend("default", backend)
        .build()
        .expect("build");
    let _ = valence; // boot helper surface remains available
}
