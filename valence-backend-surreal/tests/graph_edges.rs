//! Graph edge query tests for embedded Surreal backend.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use surrealdb::engine::local::Mem;
use valence_backend_surreal::{SDb, SurrealEmbeddedBackend};
use valence_core::compiled_query::CompiledQuery;
use valence_core::record_id::RecordId;
use valence_core::DatabaseBackend;

async fn backend() -> SurrealEmbeddedBackend {
    let db = SDb::init();
    db.connect::<Mem>(()).await.unwrap();
    db.use_ns("test").use_db("test").await.unwrap();
    SurrealEmbeddedBackend::new(db)
}

#[tokio::test]
async fn edge_count_query_with_backtick_in_out() {
    let b = backend().await;
    b.upsert_record("permission", "p1", serde_json::json!({ "name": "p" }))
        .await
        .unwrap();
    b.upsert_record(
        "user",
        "member",
        serde_json::json!({ "email": "m@example.com" }),
    )
    .await
    .unwrap();
    let from = RecordId::new("permission", "p1");
    let to = RecordId::new("user", "member");
    b.relate_edge(&from, "permission_allowed_user", &to)
        .await
        .unwrap();
    let compiled = CompiledQuery {
        query_string: "SELECT count() AS count FROM permission_allowed_user WHERE `in` = type::record($from_tb, $from_id) AND `out` = type::record($to_tb, $to_id)".to_string(),
        params: vec![
            ("from_tb".to_string(), serde_json::json!("permission")),
            ("from_id".to_string(), serde_json::json!("p1")),
            ("to_tb".to_string(), serde_json::json!("user")),
            ("to_id".to_string(), serde_json::json!("member")),
        ],
    };
    let rows = b.execute_compiled_query(&compiled).await.expect("count");
    assert_eq!(
        rows[0].get("count").and_then(|v| v.as_i64()),
        Some(1),
        "{rows:?}"
    );
}

#[tokio::test]
async fn relate_new_edge_table_name() {
    let b = backend().await;
    b.upsert_record("a", "1", serde_json::json!({}))
        .await
        .unwrap();
    b.upsert_record("b", "2", serde_json::json!({}))
        .await
        .unwrap();
    let fa = RecordId::new("a", "1");
    let tb = RecordId::new("b", "2");
    b.relate_edge(&fa, "permission_allowed_principal", &tb)
        .await
        .expect("relate new edge table");
}
