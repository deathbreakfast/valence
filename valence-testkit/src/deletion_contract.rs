//! Deletion service + dispatch integration contract.

use std::sync::Arc;

use valence_core::actor::Actor;
use valence_core::deletion::{
    dispatch, is_deletion_dispatcher_registered, DeletionRequest, DeletionService,
};
use valence_core::error::Result;
use valence_core::runtime::Valence;
use valence_core::DatabaseBackend;

use crate::deletion_capture::reset_deletion_capture;
use crate::harness_lock::lock_harness;

/// Exercise deletion persistence and dispatch capture against any backend.
pub async fn run_deletion_contract(backend: Arc<dyn DatabaseBackend>) -> Result<()> {
    let _harness_guard = lock_harness().await;
    let _ = reset_deletion_capture();
    assert!(
        is_deletion_dispatcher_registered(),
        "deletion capture dispatcher must be registered"
    );

    let v = Valence::builder()
        .add_backend("default", Arc::clone(&backend))
        .with_actor(Actor::User {
            user_id: "deletion_contract".to_string(),
        })
        .build()?;

    backend
        .ensure_schemaless_table("deletion_contract_smoke")
        .await?;
    backend
        .create_record(
            "deletion_contract_smoke",
            serde_json::json!({"id": "d1", "label": "target"}),
        )
        .await?;

    let actor_json = serde_json::to_value(v.actor()).unwrap_or(serde_json::Value::Null);
    let run_id =
        DeletionService::create_run("deletion_contract_smoke", "d1", actor_json.clone(), &v)
            .await?;

    let persisted = DeletionService::get_run_json(&run_id, &v)
        .await?
        .expect("deletion run row");
    assert_eq!(
        persisted.get("status").and_then(|v| v.as_str()),
        Some("queued")
    );
    assert_eq!(
        persisted.get("root_table").and_then(|v| v.as_str()),
        Some("deletion_contract_smoke")
    );

    dispatch(DeletionRequest {
        run_id: run_id.clone(),
        root_table: "deletion_contract_smoke".to_string(),
        root_record_id: "d1".to_string(),
        actor_json,
    })
    .await?;

    let captured = reset_deletion_capture();
    dispatch(DeletionRequest {
        run_id: "capture-check".to_string(),
        root_table: "deletion_contract_smoke".to_string(),
        root_record_id: "d1".to_string(),
        actor_json: serde_json::json!({"role": "test"}),
    })
    .await?;

    let (len, run_id) = {
        let reqs = captured
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (reqs.len(), reqs[0].run_id.clone())
    };
    drop(captured);
    assert_eq!(len, 1);
    assert_eq!(run_id, "capture-check");

    Ok(())
}

/// Mem backend deletion contract row.
pub async fn run_deletion_contract_mem() -> Result<()> {
    use valence_backend_mem::InMemoryBackend;
    run_deletion_contract(Arc::new(InMemoryBackend::new())).await
}

/// Deletion contract for any matrix storage adapter (skips when unavailable).
pub async fn run_deletion_contract_for(storage: crate::matrix::StorageAdapter) -> Result<()> {
    use crate::matrix::extended_store_available;
    use crate::model_contract::backend_for_storage;
    if !extended_store_available(storage) {
        eprintln!("deletion contract {}: skipped", storage.slug());
        return Ok(());
    }
    run_deletion_contract(backend_for_storage(storage, None).await?).await
}

/// Surreal embedded-mem deletion contract row.
#[cfg(feature = "surreal-mem")]
pub async fn run_deletion_contract_surreal_mem() -> Result<()> {
    use valence_backend_surreal::{
        connect_embedded_at_path, EmbeddedEngine, SurrealEmbeddedBackend,
    };

    let db = connect_embedded_at_path(
        EmbeddedEngine::Mem,
        "",
        "deletion_contract",
        "deletion_contract",
    )
    .await?;
    run_deletion_contract(Arc::new(SurrealEmbeddedBackend::new(db))).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use valence_core::deletion::register_noop_deletion_dispatcher_for_tests;
    use valence_core::error::Error;

    #[tokio::test]
    async fn dispatch_errors_without_registration_when_unregistered() {
        // Process-global OnceLock dispatcher: serialize against sibling harness tests.
        let _harness_guard = lock_harness().await;
        if is_deletion_dispatcher_registered() {
            // Another test in this process already installed a dispatcher.
            return;
        }
        let err = dispatch(DeletionRequest {
            run_id: "x".into(),
            root_table: "t".into(),
            root_record_id: "1".into(),
            actor_json: serde_json::Value::Null,
        })
        .await
        .expect_err("expected dispatch error");
        assert!(matches!(err, Error::Internal(_)));
    }

    #[tokio::test]
    async fn mem_deletion_contract() {
        run_deletion_contract_mem()
            .await
            .expect("mem deletion contract");
    }

    #[tokio::test]
    #[cfg(feature = "surreal-mem")]
    async fn surreal_mem_deletion_contract() {
        run_deletion_contract_surreal_mem()
            .await
            .expect("surreal-mem deletion contract");
    }

    #[tokio::test]
    async fn noop_dispatcher_satisfies_dispatch() {
        let _harness_guard = lock_harness().await;
        register_noop_deletion_dispatcher_for_tests();
        dispatch(DeletionRequest {
            run_id: "noop".into(),
            root_table: "t".into(),
            root_record_id: "1".into(),
            actor_json: serde_json::Value::Null,
        })
        .await
        .expect("noop dispatch");
    }
}
