//! Env-gated Postgres safe-tweak sync (nullability / DEFAULT).
//!
//! Skips when `DATABASE_URL` is unset or connect fails.

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

async fn connect_or_skip() -> Option<Arc<PostgresBackend>> {
    match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => Some(Arc::new(b)),
        Err(e) => {
            eprintln!("postgres connect failed: {e} — skipping");
            None
        }
    }
}

#[tokio::test]
async fn postgres_safe_tweak_set_nullable_and_default() {
    let Some(backend) = connect_or_skip().await else {
        return;
    };

    let table = format!(
        "pg_tweak_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    let v1 = StorageLayout {
        table: table.clone(),
        fields: vec![
            id_field(),
            LayoutField {
                name: "name".into(),
                storage: FieldStorage::String,
                primary_key: false,
                nullable: false,
                unique: false,
                indexed: false,
                default: None,
            record_table: None,
            },
        ],
    };
    backend.ensure_typed_table(&v1).await.expect("ensure v1");
    backend
        .create_record(
            &table,
            serde_json::json!({"id": {"table": &table, "id": "r1"}, "name": "seed"}),
        )
        .await
        .expect("seed row");

    let v2 = StorageLayout {
        table: table.clone(),
        fields: vec![
            id_field(),
            LayoutField {
                name: "name".into(),
                storage: FieldStorage::String,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
                default: Some("anon".into()),
            record_table: None,
            },
        ],
    };
    backend.sync_typed_table(&v2).await.expect("safe tweaks");
    backend
        .write_schema_version(&table, "1.1.0")
        .await
        .expect("stamp");

    let inspected = backend
        .inspect_typed_layout(&table)
        .await
        .expect("inspect")
        .expect("present");
    let name = inspected
        .fields
        .iter()
        .find(|f| f.name == "name")
        .expect("name field");
    assert!(
        name.nullable,
        "expected name nullable after SetNullable: {name:?}"
    );

    let default_txt: Option<String> = sqlx::query_scalar(
        "SELECT column_default FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = $1 AND column_name = 'name'",
    )
    .bind(&table)
    .fetch_optional(backend.pool())
    .await
    .expect("column_default query");
    let default_txt = default_txt.unwrap_or_default();
    assert!(
        default_txt.contains("anon"),
        "expected DEFAULT involving anon, got {default_txt:?}"
    );
    assert_eq!(
        backend
            .read_schema_version(&table)
            .await
            .unwrap()
            .as_deref(),
        Some("1.1.0")
    );

    let _ = sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))
        .execute(backend.pool())
        .await;
}
