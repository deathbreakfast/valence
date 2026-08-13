//! Depth-2 hop pair contract using hop-pair-model-host.

use std::sync::Arc;

use hop_pair_model_host::{Project, Task, HOP_A, HOP_B};
use valence_core::actor::Actor;
use valence_core::error::Result;
use valence_core::record_id::RecordId;
use valence_core::router::DatabaseRouter;
use valence_core::router_key::router_key;
use valence_core::runtime::Valence;
use valence_core::DatabaseBackend;
use valence_core::{Model, StringPredicate};

use crate::bootstrap::WireBackendOptions;
use crate::hops::capability::{hop_adapter_excluded, pair_nested_where_skip, HopSkip};
use crate::hops::layout::HopPair;
use crate::matrix::{extended_store_available_with_wire, StorageAdapter};
use crate::model_contract::backend_for_storage;

/// Run Project↔Task hop assertions for one directed pair.
pub async fn run_hop_pair_contract(pair: HopPair, wire: Option<&WireBackendOptions>) -> Result<()> {
    if hop_adapter_excluded(pair.primary) || hop_adapter_excluded(pair.secondary) {
        eprintln!(
            "hop pair {}: SKIP {} — acme-stub excluded from hop matrix",
            pair.slug(),
            HopSkip::BackendUnavailable.label()
        );
        return Ok(());
    }
    if !pair_available(pair, wire) {
        eprintln!(
            "hop pair {}: SKIP {} — primary/secondary not available in this environment",
            pair.slug(),
            HopSkip::BackendUnavailable.label()
        );
        return Ok(());
    }

    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");

    let primary = backend_for_storage(pair.primary, wire).await?;
    let secondary = backend_for_storage(pair.secondary, wire).await?;

    // Nested EXISTS SQL for Project→Task runs on the Project (primary) engine and
    // may reference hop_pair_task even when Task rows live on secondary. Sync both
    // typed layouts on both backends (additive) before seeding / nested WHERE —
    // CREATE IF NOT EXISTS alone leaves incomplete tables from older schemaless ensures.
    for backend in [&primary, &secondary] {
        for table in ["hop_pair_project", "hop_pair_task"] {
            if let Ok(layout) =
                valence_core::storage_layout::StorageLayout::from_registry_table(table)
            {
                DatabaseBackend::sync_typed_table(backend.as_ref(), &layout).await?;
            }
        }
    }

    let mut router = DatabaseRouter::new();
    router.register(router_key("primary", HOP_A), Arc::clone(&primary));
    router.register(router_key("secondary", HOP_B), Arc::clone(&secondary));
    let default_key = router_key("primary", HOP_A);

    let valence = Valence::builder()
        .database_router(Arc::new(router))
        .default_backend_key(default_key)
        .with_actor(Actor::System {
            operation: "hop_pair".to_string(),
        })
        .build()?;

    // Stamp only hop-pair tables — full-registry sync would refuse orphan columns left
    // on shared wire DBs by other harnesses (e.g. admin smoke extras).
    for table in ["hop_pair_project", "hop_pair_task"] {
        valence_core::storage_layout::sync_typed_table_for(&valence, table).await?;
    }

    seed_and_assert_hops(&valence, pair).await
}

fn pair_available(pair: HopPair, wire: Option<&WireBackendOptions>) -> bool {
    extended_store_available_with_wire(pair.primary, wire)
        && extended_store_available_with_wire(pair.secondary, wire)
        && pair.primary != StorageAdapter::AcmeStub
        && pair.secondary != StorageAdapter::AcmeStub
}

async fn seed_and_assert_hops(valence: &Valence, pair: HopPair) -> Result<()> {
    let project = Project::new("hop-pair".to_string()).expect("new project");
    let created = Project::create(project, valence).await?;
    let project_id = created.id().expect("project id").id().to_string();

    let task = Task::new(
        "first task".to_string(),
        RecordId::new("hop_pair_project", &project_id),
    )
    .expect("new task");
    let task_row = Task::create(task, valence).await?;
    let task_id = task_row.id().expect("task id").id().to_string();

    let loaded_project = task_row.get_project(valence).await?;
    assert_eq!(loaded_project.name(), "hop-pair");

    let tasks = Task::get_from_project(&loaded_project, valence).await?;
    assert_eq!(
        tasks.len(),
        1,
        "HasMany reverse nav must return the seeded task ({})",
        pair.slug()
    );
    assert_eq!(tasks[0].id().expect("id").id(), task_id);

    if let Some(reason) = pair_nested_where_skip(pair) {
        eprintln!(
            "hop pair {}: SKIP {} — {reason}",
            pair.slug(),
            HopSkip::NestedWhereUnsupported.label()
        );
        return Ok(());
    }

    let projects = Project::query(valence)
        .where_tasks_has_results(|q| {
            q.where_string(
                "title".to_string(),
                StringPredicate::Equals("first task".into()),
            )
        })
        .await?;
    assert!(
        !projects.is_empty(),
        "nested EXISTS must return the seeded project ({})",
        pair.slug()
    );
    assert_eq!(projects[0].id().expect("id").id(), project_id);

    let miss = Project::query(valence)
        .where_tasks_has_results(|q| {
            q.where_string(
                "title".to_string(),
                StringPredicate::Equals("no-such-task".into()),
            )
        })
        .await?;
    assert!(
        miss.is_empty(),
        "negative nested EXISTS must be empty ({})",
        pair.slug()
    );

    Ok(())
}
