//! Delete-now catalog steps.

use crate::bootstrap::BootstrapSession;
use crate::delete_now::{
    run_delete_now_cascade, run_delete_now_partial_retry_for_storage, run_delete_now_privacy_deny,
    run_delete_now_restrict,
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
        ScenarioStep::DeleteNowCascade => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            run_delete_now_cascade(valence).await
        }
        ScenarioStep::DeleteNowPrivacyDeny => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            run_delete_now_privacy_deny(valence).await
        }
        ScenarioStep::DeleteNowRestrict => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            run_delete_now_restrict(valence).await
        }
        ScenarioStep::DeleteNowCrossEnginePartialRetry => {
            run_delete_now_partial_retry_for_storage(
                session.matrix().storage,
                session.wire_options(),
            )
            .await
        }
        other => Err(format!("delete_now step mismatch: {other:?}")),
    }
}
