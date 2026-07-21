//! Adapter CRUD, compiled query, union/join IR, M2M edge smoke.

use valence_core::compiled_query::CompiledQuery;
use valence_core::query::QueryCore;
use valence_core::record_id::RecordId;
use valence_core::StringPredicate;

use crate::bootstrap::BootstrapSession;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    match step {
        ScenarioStep::CrudSmoke { table, id } => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            // Wire stores are shared across matrix rows; clear leftovers from prior runs.
            let _ = backend.delete_record(table, id).await;
            backend
                .create_record(table, serde_json::json!({"id": id, "name": "smoke"}))
                .await
                .map_err(|e| e.to_string())?;
            let fetched = backend
                .get_record(table, id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "record missing after create".to_string())?;
            if mode == RunMode::Correctness {
                assert_eq!(fetched.get("name").and_then(|v| v.as_str()), Some("smoke"));
            }
        }
        ScenarioStep::AssertGetMissing { table, id } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            let fetched = backend
                .get_record(table, id)
                .await
                .map_err(|e| e.to_string())?;
            if fetched.is_some() {
                return Err(format!("expected missing record {table}:{id}"));
            }
        }
        ScenarioStep::CompiledQueryEmpty { table } => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            let compiled = CompiledQuery::new(format!("SELECT * FROM {table} LIMIT 10"), vec![]);
            let rows = backend
                .execute_compiled_query(&compiled)
                .await
                .map_err(|e| e.to_string())?;
            if mode == RunMode::Correctness && !rows.is_empty() {
                return Err(format!(
                    "expected empty query result, got {} rows",
                    rows.len()
                ));
            }
        }
        ScenarioStep::QueryUnionJoinSmoke => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let left = QueryCore::new("project".into())
                .where_string("name".into(), StringPredicate::Equals("a".into()));
            let right = QueryCore::new("project".into())
                .where_string("name".into(), StringPredicate::Equals("b".into()));
            let joined = left.clone().join_with(right.clone());
            let unioned = left.union_with(right);
            if joined.where_clauses.len() < 2 {
                return Err("join_with should combine where clauses".into());
            }
            if unioned.or_groups.is_empty() && unioned.where_clauses.is_empty() {
                return Err("union_with should produce or_groups or clauses".into());
            }
        }
        ScenarioStep::M2mRelateSmoke => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            if !backend.capabilities().supports_graph_edges {
                return Ok(());
            }
            let table = "m2m_smoke_node";
            // Wire stores are shared across matrix rows; clear leftovers from prior runs.
            let _ = backend.delete_record(table, "a").await;
            let _ = backend.delete_record(table, "b").await;
            backend
                .create_record(table, serde_json::json!({"id": "a", "name": "a"}))
                .await
                .map_err(|e| e.to_string())?;
            backend
                .create_record(table, serde_json::json!({"id": "b", "name": "b"}))
                .await
                .map_err(|e| e.to_string())?;
            let from = RecordId::new(table, "a");
            let to = RecordId::new(table, "b");
            backend
                .relate_edge(&from, "m2m_edge", &to)
                .await
                .map_err(|e| e.to_string())?;
            let targets = backend
                .get_edge_targets(&from, "m2m_edge")
                .await
                .map_err(|e| e.to_string())?;
            if mode == RunMode::Correctness && targets.is_empty() {
                return Err("expected M2M edge targets after relate".into());
            }
            backend
                .unrelate_edge(&from, "m2m_edge", &to)
                .await
                .map_err(|e| e.to_string())?;
        }
        other => return Err(format!("crud step mismatch: {other:?}")),
    }
    Ok(())
}
