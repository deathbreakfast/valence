//! CRUD integration tests for embedded Surreal backend.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use surrealdb::engine::local::Mem;
use valence_backend_surreal::{SDb, SurrealEmbeddedBackend};
use valence_core::DatabaseBackend;
use valence_core::RecordId;

async fn backend() -> SurrealEmbeddedBackend {
    let db = SDb::init();
    db.connect::<Mem>(()).await.unwrap();
    db.use_ns("test").use_db("test").await.unwrap();
    SurrealEmbeddedBackend::new(db)
}

#[tokio::test]
async fn crud_and_edges_round_trip() {
    let b = backend().await;

    let row = b
        .create_record("widget", serde_json::json!({ "id": "w1", "name": "a" }))
        .await
        .expect("create");
    assert_eq!(row.get("name").and_then(|v| v.as_str()), Some("a"));

    let got = b.get_record("widget", "w1").await.expect("get");
    assert!(got.is_some());

    let updated = b
        .update_record(
            "widget",
            "w1",
            serde_json::json!({ "id": "widget:w1", "name": "b" }),
        )
        .await
        .expect("update");
    assert_eq!(updated.get("name").and_then(|v| v.as_str()), Some("b"));

    let up = b
        .upsert_record(
            "widget",
            "w2",
            serde_json::json!({ "id": "widget:w2", "name": "c" }),
        )
        .await
        .expect("upsert");
    assert_eq!(up.get("name").and_then(|v| v.as_str()), Some("c"));

    b.delete_record("widget", "w1").await.expect("delete");
    assert!(b.get_record("widget", "w1").await.expect("get2").is_none());

    let _ = b
        .create_record("a", serde_json::json!({ "id": "a1", "x": 1 }))
        .await;
    let _ = b
        .create_record("b", serde_json::json!({ "id": "b1", "y": 2 }))
        .await;
    let fa = RecordId::new("a", "a1");
    let tb = RecordId::new("b", "b1");
    b.relate_edge(&fa, "link", &tb).await.expect("relate");
    let outs = b.get_edge_targets(&fa, "link").await.expect("edges");
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0].table(), "b");
    assert_eq!(outs[0].id(), "b1");
    b.unrelate_edge(&fa, "link", &tb).await.expect("unrelate");
    assert!(b
        .get_edge_targets(&fa, "link")
        .await
        .expect("edges2")
        .is_empty());
}

#[tokio::test]
async fn merge_record_patches_fields() {
    let b = backend().await;
    b.upsert_record(
        "widget",
        "m1",
        serde_json::json!({ "name": "before", "count": 1 }),
    )
    .await
    .expect("upsert");
    let merged = b
        .merge_record("widget", "m1", serde_json::json!({ "count": 2 }))
        .await
        .expect("merge");
    assert_eq!(merged.get("name").and_then(|v| v.as_str()), Some("before"));
    assert_eq!(merged.get("count").and_then(|v| v.as_i64()), Some(2));
}
