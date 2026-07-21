//! Generated model CRUD, query, ownership, graph, read-cache steps.

use valence_core::ownership::{OwnershipGateStatus, OwnershipService};
use valence_core::read_cache::{invalidate, read_cache_enabled};
use valence_core::record_id::RecordId;
use valence_core::{Model, SortDirection, StringPredicate};

use product_model_host::Project;

use crate::bootstrap::BootstrapSession;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    match step {
        ScenarioStep::ModelCrudSmoke => model_crud(session, mode).await?,
        ScenarioStep::ModelUpdateUpsert => model_update_upsert(session, mode).await?,
        ScenarioStep::OwnershipGateSmoke => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            OwnershipService::apply_pending_deletion_gate(
                "catalog_ownership_smoke",
                "row1",
                OwnershipGateStatus::Status("active".to_string()),
            )
            .map_err(|e| e.to_string())?;
        }
        ScenarioStep::GraphEdgeSmoke => graph_edge(session, mode).await?,
        ScenarioStep::QueryFilterEq => query_filter_eq(session, mode).await?,
        ScenarioStep::QueryFilterMiss => query_filter_miss(session, mode).await?,
        ScenarioStep::QueryOrderBy => query_order_by(session, mode).await?,
        ScenarioStep::QueryPagination => query_pagination(session, mode).await?,
        ScenarioStep::QueryOffsetEmpty => query_offset_empty(session, mode).await?,
        ScenarioStep::ReadCacheSmoke => read_cache_smoke(session, mode).await?,
        other => return Err(format!("model step mismatch: {other:?}")),
    }
    Ok(())
}

async fn model_crud(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let project = Project::new("catalog-smoke".to_string()).map_err(|e| e.to_string())?;
    let created = Project::create(project, valence)
        .await
        .map_err(|e| e.to_string())?;
    let project_id = created.id().ok_or("missing project id")?.id();
    let fetched = Project::get(project_id, valence)
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && fetched.is_none() {
        return Err("model get returned none after create".into());
    }
    Ok(())
}

async fn model_update_upsert(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let project = Project::new("upd-upsert".to_string()).map_err(|e| e.to_string())?;
    let created = Project::create(project, valence)
        .await
        .map_err(|e| e.to_string())?;
    let id = created.id().ok_or("missing id")?.id().to_string();

    let updated = Project::new("updated-name".to_string()).map_err(|e| e.to_string())?;
    let after_update = Project::update(&id, updated, valence)
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && after_update.name() != "updated-name" {
        return Err("update did not apply name".into());
    }

    let upserted = Project::new("upserted".to_string()).map_err(|e| e.to_string())?;
    let after_upsert = Project::upsert(&id, upserted, valence)
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && after_upsert.name() != "upserted" {
        return Err("upsert did not apply name".into());
    }
    Ok(())
}

async fn graph_edge(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    if !backend.capabilities().supports_graph_edges {
        return Ok(());
    }
    let table = "graph_edge_smoke";
    // Wire stores are shared across matrix rows; clear leftovers from prior runs.
    let _ = backend.delete_record(table, "n1").await;
    let _ = backend.delete_record(table, "n2").await;
    backend
        .create_record(table, serde_json::json!({"id": "n1", "name": "left"}))
        .await
        .map_err(|e| e.to_string())?;
    backend
        .create_record(table, serde_json::json!({"id": "n2", "name": "right"}))
        .await
        .map_err(|e| e.to_string())?;
    let from = RecordId::new(table, "n1");
    let to = RecordId::new(table, "n2");
    backend
        .relate_edge(&from, "catalog_edge", &to)
        .await
        .map_err(|e| e.to_string())?;
    let targets = backend
        .get_edge_targets(&from, "catalog_edge")
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && targets.is_empty() {
        return Err("expected graph edge targets after relate".into());
    }
    backend
        .unrelate_edge(&from, "catalog_edge", &to)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn seed_named(session: &mut BootstrapSession, names: &[&str]) -> Result<String, String> {
    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let mut last_id = String::new();
    for name in names {
        let project = Project::new((*name).to_string()).map_err(|e| e.to_string())?;
        let created = Project::create(project, valence)
            .await
            .map_err(|e| e.to_string())?;
        last_id = created.id().ok_or("missing id")?.id().to_string();
    }
    Ok(last_id)
}

async fn query_filter_eq(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    // Unique names so prior catalog scenarios on shared wire DBs cannot collide.
    let tag = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let alpha = format!("alpha-filter-{tag}");
    let beta = format!("beta-filter-{tag}");
    seed_named(session, &[&alpha, &beta]).await?;
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let rows = Project::query(valence)
        .where_name(StringPredicate::Equals(alpha.clone()))
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && rows.len() != 1 {
        return Err(format!("expected 1 filter hit, got {}", rows.len()));
    }
    Ok(())
}

async fn query_filter_miss(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    seed_named(session, &["present-only"]).await?;
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let rows = Project::query(valence)
        .where_name(StringPredicate::Equals("does-not-exist".into()))
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && !rows.is_empty() {
        return Err(format!("expected empty filter miss, got {}", rows.len()));
    }
    Ok(())
}

async fn query_order_by(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    seed_named(session, &["zulu-order", "alpha-order"]).await?;
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let rows = Project::query(valence)
        .order_by_name(SortDirection::Asc)
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && rows.len() < 2 {
        return Err("expected at least 2 rows for order_by".into());
    }
    if mode == RunMode::Correctness {
        let names: Vec<_> = rows.iter().map(|p| p.name().to_string()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        if names != sorted {
            return Err(format!("order_by Asc mismatch: {names:?}"));
        }
    }
    Ok(())
}

async fn query_pagination(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    seed_named(session, &["p0", "p1", "p2"]).await?;
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let page = Project::query(valence)
        .order_by_name(SortDirection::Asc)
        .limit(2)
        .offset(0)
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && page.len() != 2 {
        return Err(format!("expected page size 2, got {}", page.len()));
    }
    Ok(())
}

async fn query_offset_empty(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    seed_named(session, &["one-row"]).await?;
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let page = Project::query(valence)
        .limit(10)
        .offset(10_000)
        .await
        .map_err(|e| e.to_string())?;
    if mode == RunMode::Correctness && !page.is_empty() {
        return Err(format!(
            "expected empty far-offset page, got {}",
            page.len()
        ));
    }
    Ok(())
}

async fn read_cache_smoke(session: &mut BootstrapSession, mode: RunMode) -> Result<(), String> {
    if mode == RunMode::Benchmark {
        return Ok(());
    }
    std::env::remove_var("VALENCE_READ_CACHE");
    if !read_cache_enabled() {
        return Err("read cache should be enabled by default".into());
    }
    let id = seed_named(session, &["cache-row"]).await?;
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let _ = Project::get(&id, valence)
        .await
        .map_err(|e| e.to_string())?;
    invalidate("project", &id);
    let again = Project::get(&id, valence)
        .await
        .map_err(|e| e.to_string())?;
    if again.is_none() {
        return Err("get after invalidate should still hit storage".into());
    }
    Ok(())
}
