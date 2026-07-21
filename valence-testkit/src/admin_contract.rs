//! Admin runtime contract: registries, entity read, privacy gate, delete queue.

use std::sync::{Arc, OnceLock};

use valence_core::actor::Actor;
use valence_core::admin_entity_delete::queue_delete_entity;
use valence_core::deletion::DeletionService;
use valence_core::error::Result;
use valence_core::evaluator::{DatabaseEvaluator, DEFAULT_IN_MEMORY};
use valence_core::privacy::{PrivacyEvaluator, PrivacyOperation, PrivacyRule};
use valence_core::privacy_policies::common;
use valence_core::query::QueryCore;
use valence_core::runtime::Valence;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules, SchemaPrivacy,
};
use valence_core::trait_registry::TraitRegistry;
use valence_core::{DatabaseBackend, OwnerRef, OwnershipService};

use crate::deletion_capture::reset_deletion_capture;
use crate::harness_lock::lock_harness;

// Link smoke schema inventory for registry assertions.
use minimal_schema as _;

fn authenticated_only_schema() -> &'static SchemaMetadata {
    static AUTH_EVAL: PrivacyRule = common::AUTHENTICATED;
    static METADATA: OnceLock<SchemaMetadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        let schema = Box::leak(Box::new(Schema {
            name: "admin_contract_auth".to_string(),
            version: "0.1.0".to_string(),
            databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "authenticated".to_string(),
                write: "authenticated".to_string(),
            },
            policies: Some(SchemaPolicies {
                read: Some(SchemaPolicyRules {
                    allow: vec![SchemaPolicyRule {
                        name: "AUTHENTICATED".to_string(),
                        description: None,
                        evaluator: Some(&AUTH_EVAL),
                    }],
                    ..SchemaPolicyRules::default()
                }),
                create: Some(SchemaPolicyRules {
                    allow: vec![SchemaPolicyRule {
                        name: "AUTHENTICATED".to_string(),
                        description: None,
                        evaluator: Some(&AUTH_EVAL),
                    }],
                    ..SchemaPolicyRules::default()
                }),
                ..SchemaPolicies::default()
            }),
            fields: vec![],
            edges: Vec::new(),
            connections: Vec::new(),
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            ttl: None,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: None,
            },
        }));
        SchemaMetadata::from_schema(schema)
    })
}

/// Run admin runtime checks against any backend (mem, surreal-mem, …).
pub async fn run_admin_contract(backend: Arc<dyn DatabaseBackend>) -> Result<()> {
    let _harness_guard = lock_harness().await;
    let dispatched = reset_deletion_capture();

    let v = Valence::builder()
        .add_backend("default", Arc::clone(&backend))
        .with_actor(Actor::User {
            user_id: "admin".to_string(),
        })
        .build()?;

    let schemas = SchemaRegistry::global().list_schemas();
    assert!(
        schemas.contains(&"smoke"),
        "expected smoke schema from minimal-schema inventory, got {schemas:?}"
    );
    let _traits = TraitRegistry::global().list_traits();

    // Wire stores are shared across adapters (postgres + hybrid use one database);
    // clear any leftover seed row so back-to-back contract runs stay isolated.
    let _ = backend.delete_record("smoke", "s1").await;
    backend
        .create_record("smoke", serde_json::json!({"id": "s1", "label": "sample"}))
        .await?;
    // A prior contract run leaves the ownership row in `pending_deletion`, which
    // makes queue_delete_entity a no-op; reset it so the delete dispatch fires.
    OwnershipService::ensure_active_ownership("smoke", "s1", OwnerRef::system(), &v).await?;

    let row = QueryCore::get_record_json("smoke", "s1", &v)
        .await?
        .expect("seeded row");
    assert_eq!(row.get("label").and_then(|v| v.as_str()), Some("sample"));

    let ids = QueryCore::latest_ids("smoke", 5, &v).await?;
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].id, "s1");

    let private = authenticated_only_schema();
    let anon = v.with_actor(Actor::Anonymous);
    let denied =
        PrivacyEvaluator::check_entity_read(private, &serde_json::json!({"id": "x"}), &anon).await;
    assert!(denied.is_err(), "anonymous read should be denied");

    let allowed =
        PrivacyEvaluator::check_entity_read(private, &serde_json::json!({"id": "x"}), &v).await;
    assert!(allowed.is_ok(), "authenticated read should be allowed");

    let anon_create = PrivacyEvaluator::check_entity_access(
        private,
        PrivacyOperation::Create,
        &serde_json::json!({"id": "x"}),
        &anon,
    )
    .await;
    assert!(anon_create.is_err(), "anonymous create should be denied");

    let missing_delete = queue_delete_entity("smoke", "does_not_exist", &v).await;
    assert!(
        missing_delete.is_ok(),
        "delete on missing record should be a no-op"
    );

    // Clear before the intentional delete so parallel harness traffic cannot inflate the count.
    dispatched
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    queue_delete_entity("smoke", "s1", &v).await?;

    // Drop the std MutexGuard before awaiting so the future stays Send.
    let run_id = {
        let runs = dispatched
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(runs.len(), 1, "delete dispatch should fire once");
        assert_eq!(runs[0].root_table, "smoke");
        assert_eq!(runs[0].root_record_id, "s1");
        runs[0].run_id.clone()
    };

    let persisted = DeletionService::get_run_json(&run_id, &v)
        .await?
        .expect("deletion run row");
    assert_eq!(
        persisted.get("status").and_then(|v| v.as_str()),
        Some("queued")
    );

    Ok(())
}

/// Mem backend admin contract row.
pub async fn run_admin_contract_mem() -> Result<()> {
    use valence_backend_mem::InMemoryBackend;
    run_admin_contract(Arc::new(InMemoryBackend::new())).await
}

/// Admin contract for any matrix storage adapter (skips when unavailable).
pub async fn run_admin_contract_for(storage: crate::matrix::StorageAdapter) -> Result<()> {
    use crate::matrix::extended_store_available;
    use crate::model_contract::backend_for_storage;
    if !storage.supports_admin_runtime() {
        eprintln!(
            "admin contract {}: skipped (no admin runtime)",
            storage.slug()
        );
        return Ok(());
    }
    if !extended_store_available(storage) {
        eprintln!("admin contract {}: skipped", storage.slug());
        return Ok(());
    }
    run_admin_contract(backend_for_storage(storage, None).await?).await
}

/// Surreal embedded-mem admin contract row.
#[cfg(feature = "surreal-mem")]
pub async fn run_admin_contract_surreal_mem() -> Result<()> {
    use valence_backend_surreal::{
        connect_embedded_at_path, EmbeddedEngine, SurrealEmbeddedBackend,
    };

    let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "admin_contract", "admin_contract")
        .await?;
    run_admin_contract(Arc::new(SurrealEmbeddedBackend::new(db))).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mem_admin_contract() {
        run_admin_contract_mem().await.expect("mem admin contract");
    }

    #[tokio::test]
    #[cfg(feature = "surreal-mem")]
    async fn surreal_mem_admin_contract() {
        run_admin_contract_surreal_mem()
            .await
            .expect("surreal-mem admin contract");
    }
}
