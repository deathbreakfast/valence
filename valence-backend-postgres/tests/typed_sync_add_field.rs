//! Ensure + additive sync adds a column without dropping existing data.
//!
//! Skips when `DATABASE_URL` is unset (local CI without Postgres).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::sync::Arc;

use valence_backend_postgres::PostgresBackend;
use valence_core::storage_layout::{FieldStorage, LayoutField, StorageLayout};
use valence_core::DatabaseBackend;

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

async fn connect() -> Option<Arc<PostgresBackend>> {
    match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => Some(Arc::new(b)),
        Err(e) => {
            eprintln!("postgres connect failed: {e} — skipping");
            None
        }
    }
}

#[tokio::test]
async fn postgres_get_decodes_by_column_name_after_added_field() {
    let Some(backend) = connect().await else {
        return;
    };
    let table = format!(
        "typed_name_decode_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let layout_v1 = StorageLayout {
        table: table.clone(),
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
            &table,
            serde_json::json!({"id": {"table": table, "id":"r1"}, "name": "alpha"}),
        )
        .await
        .expect("create");

    let mut layout_v2 = layout_v1.clone();
    layout_v2
        .fields
        .insert(1, field("score", FieldStorage::Integer, false));
    backend.sync_typed_table(&layout_v2).await.expect("sync");

    let got = backend
        .get_record(&table, "r1")
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
async fn postgres_concurrent_schema_growth_does_not_shift_gets() {
    let Some(backend) = connect().await else {
        return;
    };
    let table = format!(
        "typed_grow_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let base = StorageLayout {
        table: table.clone(),
        fields: vec![
            field("id", FieldStorage::String, true),
            field("name", FieldStorage::String, false),
        ],
    };
    backend.ensure_typed_table(&base).await.expect("ensure");
    backend
        .create_record(
            &table,
            serde_json::json!({"id": {"table": table, "id":"seed"}, "name": "seed"}),
        )
        .await
        .expect("seed");

    let mut joins = Vec::new();
    for i in 0..8 {
        let backend = Arc::clone(&backend);
        let table = table.clone();
        joins.push(tokio::spawn(async move {
            for n in 0..32 {
                let id = format!("r-{i}-{n}");
                backend
                    .create_record(
                        &table,
                        serde_json::json!({
                            "id": {"table": table, "id": id},
                            "name": format!("n{n}"),
                            "extra": n
                        }),
                    )
                    .await
                    .expect("create");
                let got = backend
                    .get_record(&table, "seed")
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
}
