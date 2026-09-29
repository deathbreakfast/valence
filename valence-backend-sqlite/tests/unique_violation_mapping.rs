//! Unique-index rejections surface as `Error::unique_violation`, even when a
//! write skips the generated pre-write probe (the losing side of a race).
#![allow(clippy::expect_used)]

use valence_backend_sqlite::SqliteBackend;
use valence_core::DatabaseBackend;

const TABLE: &str = "unique_mapping_demo";

async fn backend_with_two_rows() -> SqliteBackend {
    let backend = SqliteBackend::connect_memory()
        .await
        .expect("connect sqlite");
    backend
        .define_unique_index(TABLE, "email")
        .await
        .expect("define unique index");
    backend
        .ensure_schemaless_table(TABLE)
        .await
        .expect("ensure table");
    for (id, email) in [("u1", "a@example.com"), ("u2", "b@example.com")] {
        backend
            .create_record(TABLE, serde_json::json!({"id": id, "email": email}))
            .await
            .expect("seed row");
    }
    backend
}

#[tokio::test]
async fn index_rejection_maps_to_unique_violation_sad() {
    let backend = backend_with_two_rows().await;

    let create = backend
        .create_record(
            TABLE,
            serde_json::json!({"id": "u3", "email": "a@example.com"}),
        )
        .await
        .expect_err("duplicate create rejected");
    assert_eq!(create.as_unique_violation(), Some((TABLE, "email")));

    let update = backend
        .update_record(
            TABLE,
            "u2",
            serde_json::json!({"id": "u2", "email": "a@example.com"}),
        )
        .await
        .expect_err("duplicate update rejected");
    assert_eq!(update.as_unique_violation(), Some((TABLE, "email")));

    let merge = backend
        .merge_record(TABLE, "u2", serde_json::json!({"email": "a@example.com"}))
        .await
        .expect_err("duplicate merge rejected");
    assert_eq!(merge.as_unique_violation(), Some((TABLE, "email")));

    let row = backend
        .get_record(TABLE, "u2")
        .await
        .expect("read")
        .expect("row still present");
    assert_eq!(row["email"], "b@example.com");
}

#[tokio::test]
async fn non_unique_failures_stay_database_errors() {
    let backend = backend_with_two_rows().await;
    let missing = backend
        .update_record(
            TABLE,
            "nope",
            serde_json::json!({"id": "nope", "email": "c@example.com"}),
        )
        .await
        .expect_err("missing row");
    assert_eq!(missing.as_unique_violation(), None);
}
