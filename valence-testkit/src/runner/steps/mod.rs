//! Step dispatch — one concern per submodule (keeps cyclomatic complexity low).

mod crud;
mod model;
mod privacy;
mod telemetry;
mod wiring;

use crate::bootstrap::BootstrapSession;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run_step(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    match step {
        ScenarioStep::BuildValence
        | ScenarioStep::AssertActiveBackend
        | ScenarioStep::AssertRouterResolve { .. }
        | ScenarioStep::AssertRouterResolveFails { .. }
        | ScenarioStep::AssertRouterLen { .. }
        | ScenarioStep::AssertRouterSharedBackend { .. }
        | ScenarioStep::CrudAcrossRouterKeys { .. }
        | ScenarioStep::BuildValenceFromFactory { .. }
        | ScenarioStep::SetEnv { .. }
        | ScenarioStep::AssertEndpointResolve { .. }
        | ScenarioStep::AssertBuilderEmptyFails
        | ScenarioStep::AssertEndpointUnresolved { .. } => wiring::run(session, step, mode).await,
        ScenarioStep::CrudSmoke { .. }
        | ScenarioStep::AssertGetMissing { .. }
        | ScenarioStep::CompiledQueryEmpty { .. }
        | ScenarioStep::QueryUnionJoinSmoke
        | ScenarioStep::M2mRelateSmoke => crud::run(session, step, mode).await,
        ScenarioStep::ModelCrudSmoke
        | ScenarioStep::ModelUpdateUpsert
        | ScenarioStep::OwnershipGateSmoke
        | ScenarioStep::GraphEdgeSmoke
        | ScenarioStep::QueryFilterEq
        | ScenarioStep::QueryFilterMiss
        | ScenarioStep::QueryOrderBy
        | ScenarioStep::QueryPagination
        | ScenarioStep::QueryOffsetEmpty
        | ScenarioStep::ReadCacheSmoke => model::run(session, step, mode).await,
        ScenarioStep::AssertPrivacyReadDenied
        | ScenarioStep::AssertPrivacyWriteDenied
        | ScenarioStep::AssertValidationRejects { .. }
        | ScenarioStep::AssertValidationAccepts { .. } => privacy::run(session, step, mode).await,
        ScenarioStep::AssertTelemetryCounter { .. } => telemetry::run(session, step, mode).await,
    }
}

pub(super) fn step_label(step: &ScenarioStep) -> String {
    match step {
        ScenarioStep::BuildValence => "build_valence".into(),
        ScenarioStep::AssertActiveBackend => "assert_active_backend".into(),
        ScenarioStep::AssertRouterResolve { .. } => "assert_router_resolve".into(),
        ScenarioStep::AssertRouterResolveFails { .. } => "assert_router_resolve_fails".into(),
        ScenarioStep::AssertRouterLen { .. } => "assert_router_len".into(),
        ScenarioStep::AssertRouterSharedBackend { .. } => "assert_router_shared_backend".into(),
        ScenarioStep::CrudAcrossRouterKeys { .. } => "crud_across_router_keys".into(),
        ScenarioStep::CrudSmoke { .. } => "crud_smoke".into(),
        ScenarioStep::AssertGetMissing { .. } => "assert_get_missing".into(),
        ScenarioStep::AssertPrivacyReadDenied => "assert_privacy_read_denied".into(),
        ScenarioStep::AssertPrivacyWriteDenied => "assert_privacy_write_denied".into(),
        ScenarioStep::AssertValidationRejects { .. } => "assert_validation_rejects".into(),
        ScenarioStep::AssertValidationAccepts { .. } => "assert_validation_accepts".into(),
        ScenarioStep::ModelCrudSmoke => "model_crud_smoke".into(),
        ScenarioStep::ModelUpdateUpsert => "model_update_upsert".into(),
        ScenarioStep::OwnershipGateSmoke => "ownership_gate_smoke".into(),
        ScenarioStep::GraphEdgeSmoke => "graph_edge_smoke".into(),
        ScenarioStep::AssertTelemetryCounter { .. } => "assert_telemetry_counter".into(),
        ScenarioStep::BuildValenceFromFactory { .. } => "build_from_factory".into(),
        ScenarioStep::SetEnv { .. } => "set_env".into(),
        ScenarioStep::AssertEndpointResolve { .. } => "assert_endpoint_resolve".into(),
        ScenarioStep::AssertEndpointUnresolved { .. } => "assert_endpoint_unresolved".into(),
        ScenarioStep::CompiledQueryEmpty { .. } => "compiled_query_empty".into(),
        ScenarioStep::AssertBuilderEmptyFails => "assert_builder_empty_fails".into(),
        ScenarioStep::QueryFilterEq => "query_filter_eq".into(),
        ScenarioStep::QueryFilterMiss => "query_filter_miss".into(),
        ScenarioStep::QueryOrderBy => "query_order_by".into(),
        ScenarioStep::QueryPagination => "query_pagination".into(),
        ScenarioStep::QueryOffsetEmpty => "query_offset_empty".into(),
        ScenarioStep::ReadCacheSmoke => "read_cache_smoke".into(),
        ScenarioStep::QueryUnionJoinSmoke => "query_union_join_smoke".into(),
        ScenarioStep::M2mRelateSmoke => "m2m_relate_smoke".into(),
    }
}
