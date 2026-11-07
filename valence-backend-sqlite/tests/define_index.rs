//! `define_index` creates a real SQLite index, also on a table with no registry layout,
//! and later writes can still add their own columns.
#![allow(clippy::expect_used)]

use sqlx::Row;
use valence_backend_sqlite::SqliteBackend;
use valence_core::DatabaseBackend;

const TABLE: &str = "valence_data_ownership";

async fn index_names(backend: &SqliteBackend) -> Vec<String> {
    sqlx::query("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?")
        .bind(TABLE)
        .fetch_all(backend.pool())
        .await
        .expect("list indexes")
        .iter()
        .map(|r| r.get::<String, _>(0))
        .collect()
}

#[tokio::test]
async fn define_index_on_adhoc_table_happy() {
    let backend = SqliteBackend::connect_memory()
        .await
        .expect("connect sqlite");
    backend
        .define_index(TABLE, &["valence_model", "status"])
        .await
        .expect("define index");
    backend
        .define_index(TABLE, &["valence_model", "status"])
        .await
        .expect("second define is idempotent");
    assert!(index_names(&backend)
        .await
        .contains(&"valence_idx_valence_data_ownership_valence_model_status".to_string()));

    // A later write with columns the index didn't create still lands.
    backend
        .upsert_record(
            TABLE,
            "o1",
            serde_json::json!({
                "valence_model": "team",
                "record_id": "t1",
                "owner_id": "u1",
                "owner_type": "user",
                "status": "active",
            }),
        )
        .await
        .expect("write after index");
    let row = backend
        .get_record(TABLE, "o1")
        .await
        .expect("read")
        .expect("row");
    assert_eq!(row["owner_id"], "u1");
}

#[tokio::test]
async fn define_index_rejects_unsafe_names_sad() {
    let backend = SqliteBackend::connect_memory()
        .await
        .expect("connect sqlite");
    assert!(backend
        .define_index(TABLE, &["status; DROP"])
        .await
        .is_err());
    assert!(backend.define_index(TABLE, &[]).await.is_err());
}
