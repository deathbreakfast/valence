//! Mem COUNT(*) must honor WHERE FK equality (DeletionDag Restrict).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::{compiled_query_factory, DatabaseBackend, KnownEngines};

#[tokio::test]
async fn count_where_thing_eq_filters_by_fk() {
    let backend = Arc::new(InMemoryBackend::new());
    backend
        .create_record(
            "account",
            serde_json::json!({
                "id": "a-owner",
                "user": {"table": "user", "id": "owner"},
                "name": "owner-acct"
            }),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "account",
            serde_json::json!({
                "id": "a-persona",
                "user": {"table": "user", "id": "persona"},
                "name": "persona-acct"
            }),
        )
        .await
        .unwrap();

    let compiled = compiled_query_factory::count_where_thing_eq(
        KnownEngines::INMEMORY_MEM,
        "account",
        "user",
        "user",
        "persona",
    )
    .expect("compile");
    let rows = backend
        .execute_compiled_query(&compiled)
        .await
        .expect("count");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].as_i64(), Some(1));

    let compiled_miss = compiled_query_factory::count_where_thing_eq(
        KnownEngines::INMEMORY_MEM,
        "account",
        "user",
        "user",
        "nobody",
    )
    .expect("compile");
    let miss = backend
        .execute_compiled_query(&compiled_miss)
        .await
        .expect("count");
    assert_eq!(miss[0].as_i64(), Some(0));
}
