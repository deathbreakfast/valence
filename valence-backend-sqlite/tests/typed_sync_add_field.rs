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
                default: None,
                record_table: None,
            },
            LayoutField {
                name: "name".into(),
                storage: FieldStorage::String,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
                default: None,
                record_table: None,
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
        default: None,
        record_table: None,
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

fn field(name: &str, storage: FieldStorage, pk: bool) -> LayoutField {
    LayoutField {
        name: name.into(),
        storage,
        primary_key: pk,
        nullable: !pk,
        unique: pk,
        indexed: false,
        default: None,
        record_table: None,
    }
}

#[tokio::test]
async fn sqlite_get_decodes_by_column_name_after_added_field() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let layout_v1 = StorageLayout {
        table: "typed_name_decode".into(),
        fields: vec![
            field("id", FieldStorage::String, true),
            field("name", FieldStorage::String, false),
        ],
    };
    backend
        .ensure_typed_table(&layout_v1)
        .await
        .expect("ensure");
    backend
        .create_record(
            "typed_name_decode",
            serde_json::json!({"id": {"table":"typed_name_decode","id":"r1"}, "name": "alpha"}),
        )
        .await
        .expect("create");

    let mut layout_v2 = layout_v1.clone();
    layout_v2
        .fields
        .insert(1, field("score", FieldStorage::Integer, false));
    backend.sync_typed_table(&layout_v2).await.expect("sync");

    let got = backend
        .get_record("typed_name_decode", "r1")
        .await
        .expect("get")
        .expect("row");
    assert_eq!(
        got["name"], "alpha",
        "name must not shift onto the new column"
    );
    assert!(got.get("score").is_none() || got["score"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_concurrent_schema_growth_does_not_panic_gets() {
    let path = std::env::temp_dir().join(format!(
        "valence-schema-growth-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let backend = Arc::new(
        SqliteBackend::connect(path.to_str().expect("utf8 path"))
            .await
            .expect("connect"),
    );
    let base = StorageLayout {
        table: "typed_grow".into(),
        fields: vec![
            field("id", FieldStorage::String, true),
            field("name", FieldStorage::String, false),
        ],
    };
    backend.ensure_typed_table(&base).await.expect("ensure");
    backend
        .create_record(
            "typed_grow",
            serde_json::json!({"id": {"table":"typed_grow","id":"seed"}, "name": "seed"}),
        )
        .await
        .expect("seed");

    let mut joins = Vec::new();
    for i in 0..8 {
        let backend = Arc::clone(&backend);
        joins.push(tokio::spawn(async move {
            for n in 0..32 {
                let id = format!("r-{i}-{n}");
                backend
                    .create_record(
                        "typed_grow",
                        serde_json::json!({
                            "id": {"table":"typed_grow","id": id},
                            "name": format!("n{n}"),
                            "extra": n
                        }),
                    )
                    .await
                    .expect("create");
                let got = backend
                    .get_record("typed_grow", "seed")
                    .await
                    .expect("get")
                    .expect("row");
                assert_eq!(got["name"], "seed");
            }
        }));
    }
    for join in joins {
        join.await.expect("task");
    }
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn sqlite_missing_column_name_is_null_not_panic() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let layout = StorageLayout {
        table: "typed_sparse".into(),
        fields: vec![
            field("id", FieldStorage::String, true),
            field("name", FieldStorage::String, false),
        ],
    };
    backend.ensure_typed_table(&layout).await.expect("ensure");
    backend
        .create_record(
            "typed_sparse",
            serde_json::json!({"id": {"table":"typed_sparse","id":"r1"}, "name": "only"}),
        )
        .await
        .expect("create");
    let got = backend
        .get_record("typed_sparse", "r1")
        .await
        .expect("get")
        .expect("row");
    assert_eq!(got["name"], "only");
    assert!(got.get("nope").is_none() || got["nope"].is_null());
}
