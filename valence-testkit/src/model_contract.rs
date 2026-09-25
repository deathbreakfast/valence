//! Model contract tests for generated CRUD against any backend.

use std::sync::Arc;

use product_model_host::{Project, Task, TaskMutable};
use valence_core::actor::Actor;
use valence_core::error::Result;
use valence_core::runtime::Valence;
use valence_core::{DatabaseBackend, Model, RecordId};

use crate::bootstrap::WireBackendOptions;
use crate::deletion_capture::reset_deletion_capture;
use crate::harness_lock::lock_harness;
use crate::matrix::{
    extended_store_available_with_wire, extended_store_skip_reason_with_wire, StorageAdapter,
};

// Link generated schema inventory for registry-backed CRUD.
use product_model_host as _;

/// Build a backend instance for a matrix storage adapter.
pub async fn backend_for_storage(
    storage: StorageAdapter,
    #[cfg_attr(
        not(any(feature = "postgres", feature = "mongodb", feature = "redis")),
        allow(unused_variables)
    )]
    wire: Option<&WireBackendOptions>,
) -> Result<Arc<dyn DatabaseBackend>> {
    match storage {
        StorageAdapter::Mem => {
            use valence_backend_mem::InMemoryBackend;
            Ok(Arc::new(InMemoryBackend::new()))
        }
        StorageAdapter::Sqlite => {
            #[cfg(not(feature = "sqlite"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/sqlite".into(),
                ));
            }
            #[cfg(feature = "sqlite")]
            {
                use valence_backend_sqlite::SqliteBackend;
                Ok(Arc::new(SqliteBackend::connect_memory().await.map_err(
                    |e| valence_core::Error::Internal(e.to_string()),
                )?))
            }
        }
        StorageAdapter::Postgres => {
            #[cfg(not(feature = "postgres"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/postgres".into(),
                ));
            }
            #[cfg(feature = "postgres")]
            {
                use valence_backend_postgres::PostgresBackendBuilder;
                let builder = wire
                    .and_then(|o| o.postgres.clone())
                    .unwrap_or_else(PostgresBackendBuilder::new);
                Ok(Arc::new(
                    builder
                        .from_env_defaults()
                        .build()
                        .await
                        .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                ))
            }
        }
        StorageAdapter::MongoDb => {
            #[cfg(not(feature = "mongodb"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/mongodb".into(),
                ));
            }
            #[cfg(feature = "mongodb")]
            {
                use valence_backend_mongodb::MongoBackendBuilder;
                let builder = wire
                    .and_then(|o| o.mongodb.clone())
                    .unwrap_or_else(MongoBackendBuilder::new);
                Ok(Arc::new(
                    builder
                        .from_env_defaults()
                        .build()
                        .await
                        .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                ))
            }
        }
        StorageAdapter::IndraDb => {
            #[cfg(not(feature = "indradb"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/indradb".into(),
                ));
            }
            #[cfg(feature = "indradb")]
            {
                use valence_backend_indradb::IndradbBackend;
                Ok(Arc::new(IndradbBackend::new()))
            }
        }
        StorageAdapter::HybridIndraPg => {
            #[cfg(not(feature = "hybrid"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/hybrid".into(),
                ));
            }
            #[cfg(feature = "hybrid")]
            {
                use valence_backend_hybrid::HybridBackend;
                use valence_backend_postgres::PostgresBackendBuilder;
                let builder = wire
                    .and_then(|o| o.postgres.clone())
                    .unwrap_or_else(PostgresBackendBuilder::new);
                let primary = Arc::new(
                    builder
                        .from_env_defaults()
                        .build()
                        .await
                        .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                );
                Ok(Arc::new(
                    HybridBackend::builder()
                        .primary(primary)
                        .warm_edges(true)
                        .build()
                        .await
                        .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                ))
            }
        }
        StorageAdapter::Redis => {
            #[cfg(not(feature = "redis"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/redis".into(),
                ));
            }
            #[cfg(feature = "redis")]
            {
                use valence_backend_redis::RedisBackendBuilder;
                let builder = wire
                    .and_then(|o| o.redis.clone())
                    .unwrap_or_else(RedisBackendBuilder::new);
                Ok(Arc::new(
                    builder
                        .from_env_defaults()
                        .build()
                        .await
                        .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                ))
            }
        }
        StorageAdapter::SurrealMem => {
            #[cfg(not(feature = "surreal-mem"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/surreal-mem".into(),
                ));
            }
            #[cfg(feature = "surreal-mem")]
            {
                use valence_backend_surreal::{
                    connect_embedded_at_path, EmbeddedEngine, SurrealEmbeddedBackend,
                };
                let db = connect_embedded_at_path(
                    EmbeddedEngine::Mem,
                    "",
                    "model_contract",
                    "model_contract",
                )
                .await?;
                Ok(Arc::new(SurrealEmbeddedBackend::new(db)))
            }
        }
        StorageAdapter::SurrealRocksdb => {
            #[cfg(not(feature = "surreal-rocksdb"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/surreal-rocksdb".into(),
                ));
            }
            #[cfg(feature = "surreal-rocksdb")]
            {
                use valence_backend_surreal::{
                    connect_embedded_at_path, EmbeddedEngine, SurrealEmbeddedBackend,
                };
                let temp = tempfile::tempdir()
                    .map_err(|e| valence_core::Error::Internal(format!("tempdir: {e}")))?;
                let path = temp.path().join("surreal").to_string_lossy().into_owned();
                let db = connect_embedded_at_path(
                    EmbeddedEngine::RocksDb,
                    &path,
                    "model_contract",
                    "model_contract",
                )
                .await?;
                Ok(Arc::new(SurrealEmbeddedBackend::new(db)))
            }
        }
        StorageAdapter::AcmeStub => {
            #[cfg(not(feature = "acme-stub"))]
            {
                return Err(valence_core::Error::Internal(
                    "enable valence-testkit/acme-stub".into(),
                ));
            }
            #[cfg(feature = "acme-stub")]
            {
                use acme_valence_backend_stub::AcmeStubBackend;
                Ok(Arc::new(AcmeStubBackend::new()))
            }
        }
    }
}

/// Run model contract for one storage adapter (skips when unavailable).
pub async fn run_model_contract_for(storage: StorageAdapter) -> Result<()> {
    run_model_contract_for_with_wire(storage, None).await
}

/// Run model contract with optional wire builder options.
pub async fn run_model_contract_for_with_wire(
    storage: StorageAdapter,
    wire: Option<&WireBackendOptions>,
) -> Result<()> {
    if !extended_store_available_with_wire(storage, wire) {
        if let Some(reason) = extended_store_skip_reason_with_wire(storage, wire) {
            eprintln!("model contract {}: {reason} — skipping", storage.slug());
        }
        return Ok(());
    }
    if !storage.supports_model_runtime() {
        return Ok(());
    }
    run_model_contract(backend_for_storage(storage, wire).await?).await
}

/// CRUD roundtrip for product-model-host generated models on the given backend.
pub async fn run_model_contract(backend: Arc<dyn DatabaseBackend>) -> Result<()> {
    let _harness_guard = lock_harness().await;
    // Legacy two-trip get avoids adapter stubs that do not implement ownership bundle queries.
    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");

    let valence = Valence::builder()
        .add_backend("default", Arc::clone(&backend))
        .with_actor(Actor::System {
            operation: "model_contract".to_string(),
        })
        .build()?;

    let project = Project::new("alpha".to_string()).expect("new");
    let created = Project::create(project, &valence, valence::use_!(r"**Test:** Seeds a fixture **project** so the Valence model suite can exercise create and later reads against a known parent row. CI and developers running the suite only.")).await?;
    let project_id = created.id().expect("id").id();

    let fetched = Project::get(project_id, &valence, valence::use_!(r"**Test:** Reloads the fixture **project** by id so the Valence model suite can assert create persisted the expected name. CI and developers running the suite only.")).await?;
    assert!(fetched.is_some());
    let fetched = fetched.expect("row exists");

    let merged = fetched
        .get_mutable(&valence, valence::use_!(r"**Test:** Loads the fixture **project** as a mutable builder so the Valence model suite can assert sparse partial-update behavior. CI and developers running the suite only."))
        .set_name("beta".to_string())
        .expect("set_name")
        .commit()
        .await?;
    assert_eq!(merged.name(), "beta");

    // Disjoint-field concurrent-write regression: two "sessions" that each read the
    // same original row and each change a DIFFERENT field must not clobber each
    // other, even though neither knows about the other's write. This is the
    // structural property the sparse `update_with_before` write depends on — a
    // full-row replace (the pre-fix behavior) would fail this assertion.
    let project_b = Project::create(
        Project::new("gamma".to_string()).expect("new"),
        &valence,
        valence::use_!(r"**Test:** Seeds a second fixture **project** so the disjoint-field regression test has an independent value to swap a task's reference to. CI and developers running the suite only."),
    )
    .await?;
    let project_b_id = project_b.id().expect("id").id().to_string();

    let task = Task::new("ship".to_string(), RecordId::new("project", project_id)).expect("new");
    let created_task = Task::create(
        task,
        &valence,
        valence::use_!(r"**Test:** Seeds a fixture **task** so the disjoint-field regression test has a row with two independent fields to race edits against. CI and developers running the suite only."),
    )
    .await?;
    let task_id = created_task.id().expect("id").id().to_string();

    let before_task = Task::get(
        &task_id,
        &valence,
        valence::use_!(r"**Test:** Reloads the fixture **task** so both simulated concurrent editors start from the same original snapshot. CI and developers running the suite only."),
    )
    .await?
    .expect("row exists");

    TaskMutable::new(before_task.clone(), &valence)
        .set_title("shipped".to_string())
        .expect("set_title")
        .commit()
        .await?;

    TaskMutable::new(before_task.clone(), &valence)
        .set_project(RecordId::new("project", project_b_id.clone()))
        .expect("set_project")
        .commit()
        .await?;

    let final_task = Task::get(
        &task_id,
        &valence,
        valence::use_!(r"**Test:** Reloads the fixture **task** so the suite can assert both concurrent edits survived without clobbering each other. CI and developers running the suite only."),
    )
    .await?
    .expect("row exists");
    assert_eq!(final_task.title(), "shipped");
    assert_eq!(final_task.project().id(), project_b_id.as_str());

    let captured = reset_deletion_capture();
    Project::delete(project_id, &valence, valence::use_!(r"**Test:** Queues deletion of the fixture **project** so the Valence model suite can assert the cascade dispatcher path. CI and developers running the suite only.")).await?;
    assert!(!captured
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_empty());
    Ok(())
}

/// Mem backend model contract row.
pub async fn run_model_contract_mem() -> Result<()> {
    use valence_backend_mem::InMemoryBackend;
    run_model_contract(Arc::new(InMemoryBackend::new())).await
}

/// Surreal embedded-mem model contract row.
#[cfg(feature = "surreal-mem")]
pub async fn run_model_contract_surreal_mem() -> Result<()> {
    use valence_backend_surreal::{
        connect_embedded_at_path, EmbeddedEngine, SurrealEmbeddedBackend,
    };

    let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "model_contract", "model_contract")
        .await?;
    run_model_contract(Arc::new(SurrealEmbeddedBackend::new(db))).await
}

/// Acme stub model contract row.
#[cfg(feature = "acme-stub")]
pub async fn run_model_contract_acme_stub() -> Result<()> {
    use acme_valence_backend_stub::AcmeStubBackend;
    run_model_contract(Arc::new(AcmeStubBackend::new())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn model_contract_mem_green() {
        run_model_contract_mem().await.expect("model contract");
    }

    #[tokio::test]
    #[cfg(feature = "surreal-mem")]
    async fn model_contract_surreal_mem_green() {
        run_model_contract_surreal_mem()
            .await
            .expect("surreal model contract");
    }

    #[tokio::test]
    #[cfg(feature = "acme-stub")]
    async fn model_contract_acme_stub_green() {
        run_model_contract_acme_stub()
            .await
            .expect("acme model contract");
    }
}
