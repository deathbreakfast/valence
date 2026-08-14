//! Shared [`DatabaseBackend`] port contract for adapter adapters.

use std::sync::Arc;

use valence_core::backend::DatabaseBackend;
use valence_core::compiled_query::CompiledQuery;
use valence_core::record_id::RecordId;
use valence_core::Result;

const CONTRACT_TABLE: &str = "valence_backend_contract";

/// Minimal port contract every storage adapter should satisfy before matrix E2E.
pub async fn run_backend_contract(backend: Arc<dyn DatabaseBackend>) -> Result<()> {
    get_missing_record_returns_none(backend.as_ref()).await?;
    create_get_roundtrip(backend.as_ref()).await?;
    update_merge_upsert(backend.as_ref()).await?;
    delete_idempotent(backend.as_ref()).await?;
    compiled_query_select(backend.as_ref()).await?;
    relate_unrelate_edges(backend.as_ref()).await?;
    define_unique_index_idempotent(backend.as_ref()).await?;
    duplicate_unique_index_rejected(backend.as_ref()).await?;
    ensure_schemaless_table_lazy(backend.as_ref()).await?;
    Ok(())
}

async fn get_missing_record_returns_none(backend: &dyn DatabaseBackend) -> Result<()> {
    backend.ensure_schemaless_table(CONTRACT_TABLE).await?;
    assert!(
        backend
            .get_record(CONTRACT_TABLE, "missing_contract_id")
            .await?
            .is_none(),
        "missing record should return None"
    );
    Ok(())
}

async fn create_get_roundtrip(backend: &dyn DatabaseBackend) -> Result<()> {
    let created = backend
        .create_record(
            CONTRACT_TABLE,
            serde_json::json!({"id": "c1", "name": "alpha"}),
        )
        .await?;
    assert_eq!(created.get("name").and_then(|v| v.as_str()), Some("alpha"));

    let fetched = backend
        .get_record(CONTRACT_TABLE, "c1")
        .await?
        .expect("record exists");
    assert_eq!(fetched.get("name").and_then(|v| v.as_str()), Some("alpha"));
    Ok(())
}

async fn update_merge_upsert(backend: &dyn DatabaseBackend) -> Result<()> {
    let caps = backend.capabilities();
    backend
        .update_record(
            CONTRACT_TABLE,
            "c1",
            serde_json::json!({"id": "c1", "name": "beta", "score": 1}),
        )
        .await?;

    if caps.supports_merge {
        let merged = backend
            .merge_record(CONTRACT_TABLE, "c1", serde_json::json!({"score": 2}))
            .await?;
        assert_eq!(merged.get("score").and_then(|v| v.as_i64()), Some(2));
    }

    let upserted = backend
        .upsert_record(
            CONTRACT_TABLE,
            "c2",
            serde_json::json!({"id": "c2", "name": "gamma"}),
        )
        .await?;
    assert_eq!(upserted.get("name").and_then(|v| v.as_str()), Some("gamma"));
    Ok(())
}

async fn delete_idempotent(backend: &dyn DatabaseBackend) -> Result<()> {
    backend.delete_record(CONTRACT_TABLE, "c1").await?;
    assert!(backend.get_record(CONTRACT_TABLE, "c1").await?.is_none());
    backend.delete_record(CONTRACT_TABLE, "c1").await?;
    Ok(())
}

async fn compiled_query_select(backend: &dyn DatabaseBackend) -> Result<()> {
    // Seed an INTEGER-shaped datetime so SELECT * maps prove numeric JSON (not
    // stringified unix seconds — the SQLite map_select_row regression).
    let _ = backend
        .create_record(
            CONTRACT_TABLE,
            serde_json::json!({
                "id": "cq1",
                "name": "select-probe",
                "at": 1_700_000_000_i64
            }),
        )
        .await?;
    let compiled = CompiledQuery::new(format!("SELECT * FROM {CONTRACT_TABLE} LIMIT 10"), vec![]);
    let rows = backend.execute_compiled_query(&compiled).await?;
    assert!(
        !rows.is_empty(),
        "compiled SELECT * should return seeded rows"
    );
    let row = rows
        .iter()
        .find(|r| {
            r.get("id").and_then(|v| v.as_str()) == Some("cq1")
                || r.get("id")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
                    == Some("cq1")
        })
        .or_else(|| rows.first())
        .expect("row");
    if let Some(at) = row.get("at") {
        assert!(
            at.is_i64() || at.as_u64().is_some(),
            "datetime unix seconds must be JSON number, got {at:?}"
        );
        assert_eq!(
            at.as_i64().or_else(|| at.as_u64().map(|u| u as i64)),
            Some(1_700_000_000)
        );
    }
    Ok(())
}

async fn relate_unrelate_edges(backend: &dyn DatabaseBackend) -> Result<()> {
    if !backend.capabilities().supports_graph_edges {
        return Ok(());
    }

    let _ = backend
        .create_record(
            CONTRACT_TABLE,
            serde_json::json!({"id": "n1", "name": "left"}),
        )
        .await?;
    let _ = backend
        .create_record(
            CONTRACT_TABLE,
            serde_json::json!({"id": "n2", "name": "right"}),
        )
        .await?;

    let from = RecordId::new(CONTRACT_TABLE, "n1");
    let to = RecordId::new(CONTRACT_TABLE, "n2");
    backend.relate_edge(&from, "contract_edge", &to).await?;
    let targets = backend.get_edge_targets(&from, "contract_edge").await?;
    assert!(!targets.is_empty());
    backend.unrelate_edge(&from, "contract_edge", &to).await?;
    Ok(())
}

async fn define_unique_index_idempotent(backend: &dyn DatabaseBackend) -> Result<()> {
    match backend.define_unique_index(CONTRACT_TABLE, "name").await {
        Ok(()) => {
            backend.define_unique_index(CONTRACT_TABLE, "name").await?;
        }
        Err(e) if e.to_string().contains("not supported") => {}
        Err(e) => return Err(e),
    }
    Ok(())
}

async fn duplicate_unique_index_rejected(backend: &dyn DatabaseBackend) -> Result<()> {
    if backend
        .define_unique_index("contract_unique_sad", "email")
        .await
        .is_err()
    {
        return Ok(());
    }

    backend
        .ensure_schemaless_table("contract_unique_sad")
        .await?;
    backend
        .create_record(
            "contract_unique_sad",
            serde_json::json!({"id": "u1", "email": "dup@example.com"}),
        )
        .await?;
    let duplicate = backend
        .create_record(
            "contract_unique_sad",
            serde_json::json!({"id": "u2", "email": "dup@example.com"}),
        )
        .await;
    assert!(
        duplicate.is_err(),
        "duplicate unique index value should be rejected"
    );
    Ok(())
}

async fn ensure_schemaless_table_lazy(backend: &dyn DatabaseBackend) -> Result<()> {
    backend
        .ensure_schemaless_table("lazy_contract_table")
        .await?;
    let _ = backend
        .create_record("lazy_contract_table", serde_json::json!({"id": "lazy1"}))
        .await?;
    Ok(())
}
