//! `apply_batch` applies every write or none of them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::{DatabaseBackend, Error, Valence, WriteOp};

const TABLE: &str = "batch_item";

fn create(id: &str, n: i64) -> WriteOp {
    WriteOp::Create {
        table: TABLE.into(),
        content: json!({"id": id, "n": n}),
    }
}

#[tokio::test]
async fn mixed_batch_applies_in_order() {
    let backend = InMemoryBackend::new();
    backend
        .create_record(TABLE, json!({"id": "old", "n": 0}))
        .await
        .unwrap();

    let rows = backend
        .apply_batch(vec![
            create("a", 1),
            WriteOp::Update {
                table: TABLE.into(),
                id: "a".into(),
                content: json!({"id": "a", "n": 2}),
            },
            WriteOp::Merge {
                table: TABLE.into(),
                id: "a".into(),
                patch: json!({"tag": "x"}),
            },
            WriteOp::Upsert {
                table: TABLE.into(),
                id: "b".into(),
                content: json!({"n": 3}),
            },
            WriteOp::Delete {
                table: TABLE.into(),
                id: "old".into(),
            },
        ])
        .await
        .unwrap();

    assert_eq!(rows.len(), 5);
    assert_eq!(rows[2]["n"], 2);
    assert_eq!(rows[2]["tag"], "x");
    assert!(rows[4].is_null());
    assert_eq!(
        backend.get_record(TABLE, "a").await.unwrap().unwrap()["tag"],
        "x"
    );
    assert!(backend.get_record(TABLE, "b").await.unwrap().is_some());
    assert!(backend.get_record(TABLE, "old").await.unwrap().is_none());
}

#[tokio::test]
async fn failing_op_rolls_back_the_whole_batch() {
    let backend = InMemoryBackend::new();
    backend
        .create_record(TABLE, json!({"id": "keep", "n": 1}))
        .await
        .unwrap();

    let err = backend
        .apply_batch(vec![
            create("new", 9),
            WriteOp::Delete {
                table: TABLE.into(),
                id: "keep".into(),
            },
            WriteOp::Update {
                table: TABLE.into(),
                id: "missing".into(),
                content: json!({"n": 0}),
            },
        ])
        .await
        .unwrap_err();

    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    assert!(backend.get_record(TABLE, "new").await.unwrap().is_none());
    assert_eq!(
        backend.get_record(TABLE, "keep").await.unwrap().unwrap()["n"],
        1
    );
}

#[tokio::test]
async fn empty_batch_is_ok() {
    let backend = InMemoryBackend::new();
    assert_eq!(backend.apply_batch(Vec::new()).await.unwrap().len(), 0);
}

#[tokio::test]
async fn valence_apply_batch_routes_to_backend() {
    let backend = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .unwrap();

    assert_eq!(valence.apply_batch(Vec::new()).await.unwrap().len(), 0);
    valence
        .apply_batch(vec![create("v1", 1), create("v2", 2)])
        .await
        .unwrap();
    assert!(backend.get_record(TABLE, "v2").await.unwrap().is_some());

    valence
        .apply_batch(vec![
            create("v3", 3),
            WriteOp::Update {
                table: TABLE.into(),
                id: "nope".into(),
                content: json!({}),
            },
        ])
        .await
        .unwrap_err();
    assert!(backend.get_record(TABLE, "v3").await.unwrap().is_none());
}

#[tokio::test]
async fn mem_backend_advertises_transactions() {
    assert!(InMemoryBackend::new().capabilities().supports_transactions);
}
