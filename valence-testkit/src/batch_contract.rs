//! Shared contract for backends whose `apply_batch` is atomic (`supports_transactions`).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use valence_core::backend::{DatabaseBackend, WriteOp};
use valence_core::{Error, Result};

/// Table name no earlier run has created, so the contract also covers first-write DDL.
fn fresh_table(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{prefix}_{nanos}")
}

/// Check that a batch applies every op in order, or none of them when one fails.
///
/// Uses fresh table names, so shared databases can be reused across runs.
///
/// # Errors
///
/// Returns the backend error from setup or from a batch expected to succeed; assertion
/// failures panic.
pub async fn run_apply_batch_contract(backend: Arc<dyn DatabaseBackend>) -> Result<()> {
    let backend = backend.as_ref();
    assert!(
        backend.capabilities().supports_transactions,
        "{} must report supports_transactions",
        backend.engine_id()
    );
    assert_eq!(
        backend.apply_batch(Vec::new()).await?,
        Vec::<serde_json::Value>::new()
    );

    // Every op kind, in order, in one batch.
    let table = fresh_table("batch_mixed");
    backend
        .create_record(&table, json!({"id": "old", "n": 0}))
        .await?;
    let rows = backend
        .apply_batch(vec![
            WriteOp::Create {
                table: table.clone(),
                content: json!({"id": "a", "n": 1}),
            },
            WriteOp::Update {
                table: table.clone(),
                id: "a".into(),
                content: json!({"id": "a", "n": 2}),
            },
            WriteOp::Merge {
                table: table.clone(),
                id: "a".into(),
                patch: json!({"tag": "x"}),
            },
            WriteOp::Upsert {
                table: table.clone(),
                id: "b".into(),
                content: json!({"n": 3}),
            },
            WriteOp::Delete {
                table: table.clone(),
                id: "old".into(),
            },
        ])
        .await?;
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[2]["n"], 2);
    assert_eq!(rows[2]["tag"], "x");
    assert!(rows[4].is_null());
    let a = backend.get_record(&table, "a").await?.expect("a committed");
    assert_eq!((a["n"].clone(), a["tag"].clone()), (json!(2), json!("x")));
    let b = backend.get_record(&table, "b").await?.expect("b committed");
    assert_eq!(b["n"], 3);
    assert!(backend.get_record(&table, "old").await?.is_none());

    // A failing op rolls back the ops before it.
    let err = backend
        .apply_batch(vec![
            WriteOp::Delete {
                table: table.clone(),
                id: "a".into(),
            },
            WriteOp::Update {
                table: table.clone(),
                id: "missing".into(),
                content: json!({"id": "missing", "n": 9}),
            },
        ])
        .await
        .expect_err("update of a missing row fails the batch");
    assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    assert!(
        backend.get_record(&table, "a").await?.is_some(),
        "delete before the failure rolled back"
    );

    // A rolled-back batch that created the table leaves no trace, and the next write on
    // that table creates it again instead of trusting a cached layout.
    let fresh = fresh_table("batch_fresh");
    backend
        .apply_batch(vec![
            WriteOp::Create {
                table: fresh.clone(),
                content: json!({"id": "f1", "n": 1}),
            },
            WriteOp::Update {
                table: fresh.clone(),
                id: "missing".into(),
                content: json!({"id": "missing", "n": 9}),
            },
        ])
        .await
        .expect_err("update of a missing row fails the batch");
    assert!(backend.get_record(&fresh, "f1").await?.is_none());
    backend
        .create_record(&fresh, json!({"id": "f2", "n": 2}))
        .await?;
    assert_eq!(backend.get_record(&fresh, "f2").await?.expect("f2")["n"], 2);
    Ok(())
}
