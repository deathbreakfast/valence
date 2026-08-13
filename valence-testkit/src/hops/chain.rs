//! Depth-3/4 nested hop contracts using hop-chain-model-host.

use std::sync::Arc;

use hop_chain_model_host::{Note, Org, Project, Task, HOP_A, HOP_B, HOP_C, HOP_D};
use valence_core::actor::Actor;
use valence_core::error::Result;
use valence_core::query::QueryCore;
use valence_core::record_id::RecordId;
use valence_core::router::DatabaseRouter;
use valence_core::router_key::router_key;
use valence_core::runtime::Valence;
use valence_core::{Model, StringPredicate};

use crate::bootstrap::WireBackendOptions;
use crate::hops::capability::{
    hop_adapter_excluded, quad_nested_where_skip, triple_nested_where_skip, HopSkip,
};
use crate::hops::layout::{HopQuad, HopTriple};
use crate::matrix::extended_store_available_with_wire;
use crate::model_contract::backend_for_storage;

/// Depth-3 Org→Project→Task nested where exists.
pub async fn run_hop_chain_contract(
    triple: HopTriple,
    wire: Option<&WireBackendOptions>,
) -> Result<()> {
    if hop_adapter_excluded(triple.a)
        || hop_adapter_excluded(triple.b)
        || hop_adapter_excluded(triple.c)
    {
        eprintln!(
            "hop triple {}: SKIP {} — acme-stub excluded from hop matrix",
            triple.slug(),
            HopSkip::BackendUnavailable.label()
        );
        return Ok(());
    }
    if !(extended_store_available_with_wire(triple.a, wire)
        && extended_store_available_with_wire(triple.b, wire)
        && extended_store_available_with_wire(triple.c, wire))
    {
        eprintln!(
            "hop triple {}: SKIP {} — one or more engines unavailable",
            triple.slug(),
            HopSkip::BackendUnavailable.label()
        );
        return Ok(());
    }

    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
    let a = backend_for_storage(triple.a, wire).await?;
    let b = backend_for_storage(triple.b, wire).await?;
    let c = backend_for_storage(triple.c, wire).await?;
    let d = backend_for_storage(crate::matrix::StorageAdapter::Mem, wire).await?;

    let valence = build_chain_valence(a, b, c, d).await?;
    seed_depth3(&valence, triple).await
}

/// Depth-4 Org→Project→Task→Note nested where exists.
pub async fn run_hop_quad_contract(quad: HopQuad, wire: Option<&WireBackendOptions>) -> Result<()> {
    if hop_adapter_excluded(quad.a)
        || hop_adapter_excluded(quad.b)
        || hop_adapter_excluded(quad.c)
        || hop_adapter_excluded(quad.d)
    {
        eprintln!(
            "hop quad {}: SKIP {} — acme-stub excluded from hop matrix",
            quad.slug(),
            HopSkip::BackendUnavailable.label()
        );
        return Ok(());
    }
    if !(extended_store_available_with_wire(quad.a, wire)
        && extended_store_available_with_wire(quad.b, wire)
        && extended_store_available_with_wire(quad.c, wire)
        && extended_store_available_with_wire(quad.d, wire))
    {
        eprintln!(
            "hop quad {}: SKIP {} — one or more engines unavailable",
            quad.slug(),
            HopSkip::BackendUnavailable.label()
        );
        return Ok(());
    }

    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
    let a = backend_for_storage(quad.a, wire).await?;
    let b = backend_for_storage(quad.b, wire).await?;
    let c = backend_for_storage(quad.c, wire).await?;
    let d = backend_for_storage(quad.d, wire).await?;
    let valence = build_chain_valence(a, b, c, d).await?;
    seed_depth4(&valence, quad).await
}

async fn build_chain_valence(
    a: Arc<dyn valence_core::DatabaseBackend>,
    b: Arc<dyn valence_core::DatabaseBackend>,
    c: Arc<dyn valence_core::DatabaseBackend>,
    d: Arc<dyn valence_core::DatabaseBackend>,
) -> Result<Valence> {
    let mut router = DatabaseRouter::new();
    router.register(router_key("n1", HOP_A), a);
    router.register(router_key("n2", HOP_B), b);
    router.register(router_key("n3", HOP_C), c);
    router.register(router_key("n4", HOP_D), d);
    let valence = Valence::builder()
        .database_router(Arc::new(router))
        .default_backend_key(router_key("n1", HOP_A))
        .with_actor(Actor::System {
            operation: "hop_chain".to_string(),
        })
        .build()?;
    // Scope to chain tables only — shared wire DBs may hold orphans from other harnesses.
    for table in [
        "hop_chain_org",
        "hop_chain_project",
        "hop_chain_task",
        "hop_chain_note",
    ] {
        valence_core::storage_layout::sync_typed_table_for(&valence, table).await?;
    }
    Ok(valence)
}

fn task_title_subquery(title: &str) -> QueryCore {
    QueryCore::new("hop_chain_task".into())
        .where_string("title".into(), StringPredicate::Equals(title.into()))
}

fn project_with_task_title(title: &str) -> QueryCore {
    QueryCore::new("hop_chain_project".into()).where_connection_exists_reverse(
        "hop_chain_task".into(),
        "project".into(),
        task_title_subquery(title),
    )
}

fn note_body_subquery(body: &str) -> QueryCore {
    QueryCore::new("hop_chain_note".into())
        .where_string("body".into(), StringPredicate::Equals(body.into()))
}

fn task_with_note_body(body: &str) -> QueryCore {
    QueryCore::new("hop_chain_task".into()).where_connection_exists_reverse(
        "hop_chain_note".into(),
        "task".into(),
        note_body_subquery(body),
    )
}

fn project_with_note_body(body: &str) -> QueryCore {
    QueryCore::new("hop_chain_project".into()).where_connection_exists_reverse(
        "hop_chain_task".into(),
        "project".into(),
        task_with_note_body(body),
    )
}

async fn seed_depth3(valence: &Valence, triple: HopTriple) -> Result<()> {
    let org = Org::new("acme".to_string()).expect("org");
    let org_row = Org::create(org, valence).await?;
    let org_id = org_row.id().expect("id").id().to_string();

    let project =
        Project::new("alpha".to_string(), RecordId::new("hop_chain_org", &org_id)).expect("p");
    let project_row = Project::create(project, valence).await?;
    let project_id = project_row.id().expect("id").id().to_string();

    let task = Task::new(
        "ship".to_string(),
        RecordId::new("hop_chain_project", &project_id),
    )
    .expect("t");
    let _task_row = Task::create(task, valence).await?;

    // Always assert seed + reverse nav (routing), independent of nested EXISTS support.
    let projects = Project::query(valence)
        .where_name(StringPredicate::Equals("alpha".into()))
        .await?;
    assert_eq!(
        projects.len(),
        1,
        "hop triple {}: expected seeded project",
        triple.slug()
    );
    let tasks = Task::get_from_project(&projects[0], valence).await?;
    assert_eq!(
        tasks.len(),
        1,
        "hop triple {}: reverse nav must return seeded task",
        triple.slug()
    );

    if let Some(reason) = triple_nested_where_skip(triple) {
        eprintln!(
            "hop triple {}: SKIP {} — {reason}",
            triple.slug(),
            HopSkip::NestedWhereUnsupported.label()
        );
        return Ok(());
    }

    let orgs = Org::query(valence)
        .where_projects_has_results(|_| project_with_task_title("ship"))
        .await?;
    assert!(
        !orgs.is_empty(),
        "hop triple {}: nested EXISTS must return org",
        triple.slug()
    );
    assert_eq!(orgs[0].name(), "acme");

    let miss = Org::query(valence)
        .where_projects_has_results(|_| project_with_task_title("missing"))
        .await?;
    assert!(
        miss.is_empty(),
        "hop triple {}: negative nested EXISTS must be empty",
        triple.slug()
    );
    Ok(())
}

async fn seed_depth4(valence: &Valence, quad: HopQuad) -> Result<()> {
    let triple = HopTriple {
        a: quad.a,
        b: quad.b,
        c: quad.c,
    };
    seed_depth3(valence, triple).await?;

    let projects = Project::query(valence)
        .where_name(StringPredicate::Equals("alpha".into()))
        .await?;
    assert_eq!(
        projects.len(),
        1,
        "hop quad {}: expected project after depth3",
        quad.slug()
    );
    let tasks = Task::get_from_project(&projects[0], valence).await?;
    assert!(
        !tasks.is_empty(),
        "hop quad {}: expected task via reverse nav",
        quad.slug()
    );
    let task_id = tasks[0].id().expect("id").id().to_string();

    let note = Note::new(
        "todo".to_string(),
        RecordId::new("hop_chain_task", &task_id),
    )
    .expect("n");
    Note::create(note, valence)
        .await
        .unwrap_or_else(|e| panic!("hop quad {}: note create failed: {e}", quad.slug()));

    if let Some(reason) = quad_nested_where_skip(quad) {
        eprintln!(
            "hop quad {}: SKIP {} — {reason}",
            quad.slug(),
            HopSkip::NestedWhereUnsupported.label()
        );
        return Ok(());
    }

    let orgs = Org::query(valence)
        .where_projects_has_results(|_| project_with_note_body("todo"))
        .await?;
    assert_eq!(
        orgs.len(),
        1,
        "hop quad {}: nested EXISTS must return org",
        quad.slug()
    );
    Ok(())
}
