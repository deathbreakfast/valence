//! OnDelete catalog steps (same-engine + cross-engine soft-skip).

use crate::bootstrap::BootstrapSession;
use crate::hops::HopPair;
use crate::on_delete::{
    on_delete_cross_engine_secondary, run_on_delete_cascade_cross_engine,
    run_on_delete_cascade_same_backend, run_on_delete_remove_edge, run_on_delete_restrict_blocks,
    run_on_delete_set_null, run_on_delete_set_null_cross_engine,
};
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    if mode == RunMode::Benchmark {
        return Ok(());
    }
    match step {
        ScenarioStep::OnDeleteCascadeSameBackend => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            run_on_delete_cascade_same_backend(valence).await
        }
        ScenarioStep::OnDeleteSetNull => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            run_on_delete_set_null(valence).await
        }
        ScenarioStep::OnDeleteRemoveEdge => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            run_on_delete_remove_edge(valence).await
        }
        ScenarioStep::OnDeleteRestrictBlocks => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            run_on_delete_restrict_blocks(valence).await
        }
        ScenarioStep::OnDeleteCascadeCrossEngine => {
            let primary = session.matrix().storage;
            let Some(secondary) = on_delete_cross_engine_secondary(primary) else {
                return Ok(());
            };
            let pair = HopPair { primary, secondary };
            run_on_delete_cascade_cross_engine(pair, session.wire_options()).await
        }
        ScenarioStep::OnDeleteSetNullCrossEngine => {
            let primary = session.matrix().storage;
            let Some(secondary) = on_delete_cross_engine_secondary(primary) else {
                return Ok(());
            };
            let pair = HopPair { primary, secondary };
            run_on_delete_set_null_cross_engine(pair, session.wire_options()).await
        }
        other => Err(format!("on_delete step mismatch: {other:?}")),
    }
}
