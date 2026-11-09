//! Mem ORDER BY sorts numbers numerically and honors every key.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use valence_backend_mem::InMemoryBackend;
use valence_core::{CompiledQuery, DatabaseBackend};

#[tokio::test]
async fn order_by_created_at_desc_then_id() {
    let backend = InMemoryBackend::new();
    for (id, created_at) in [("a", 9), ("b", 100), ("c", 100), ("d", 20)] {
        backend
            .create_record(
                "note",
                serde_json::json!({"id": id, "created_at": created_at}),
            )
            .await
            .unwrap();
    }
    let compiled = CompiledQuery::new(
        "SELECT * FROM note ORDER BY created_at DESC, id ASC LIMIT 3".into(),
        vec![],
    );
    let rows = backend.execute_compiled_query(&compiled).await.unwrap();
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["b", "c", "d"]);
}
