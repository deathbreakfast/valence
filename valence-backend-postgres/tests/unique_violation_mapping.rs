//! Unique-index rejections surface as `Error::unique_violation` on Postgres,
//! even when a write skips the generated pre-write probe.
//!
//! Skips when `DATABASE_URL` is unset (local CI without Postgres).

#![allow(clippy::expect_used, clippy::print_stderr)]

use valence_backend_postgres::PostgresBackend;
use valence_core::DatabaseBackend;

async fn connect() -> Option<PostgresBackend> {
    match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("postgres connect failed: {e} — skipping");
            None
        }
    }
}

#[tokio::test]
async fn index_rejection_maps_to_unique_violation_sad() {
    let Some(backend) = connect().await else {
        return;
    };
    let table = format!(
        "unique_mapping_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    backend
        .define_unique_index(&table, "email")
        .await
        .expect("define unique index");
    backend
        .ensure_schemaless_table(&table)
        .await
        .expect("ensure table");
    for (id, email) in [("u1", "a@example.com"), ("u2", "b@example.com")] {
        backend
            .create_record(&table, serde_json::json!({"id": id, "email": email}))
            .await
            .expect("seed row");
    }

    let create = backend
        .create_record(
            &table,
            serde_json::json!({"id": "u3", "email": "a@example.com"}),
        )
        .await
        .expect_err("duplicate create rejected");
    assert_eq!(
        create.as_unique_violation(),
        Some((table.as_str(), "email"))
    );

    let update = backend
        .update_record(
            &table,
            "u2",
            serde_json::json!({"id": "u2", "email": "a@example.com"}),
        )
        .await
        .expect_err("duplicate update rejected");
    assert_eq!(
        update.as_unique_violation(),
        Some((table.as_str(), "email"))
    );

    let merge = backend
        .merge_record(&table, "u2", serde_json::json!({"email": "a@example.com"}))
        .await
        .expect_err("duplicate merge rejected");
    assert_eq!(merge.as_unique_violation(), Some((table.as_str(), "email")));
}
