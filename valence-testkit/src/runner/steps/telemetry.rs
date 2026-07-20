//! Telemetry assertion steps.

use crate::bootstrap::BootstrapSession;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run(
    session: &BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    match step {
        ScenarioStep::AssertTelemetryCounter {
            name,
            label_key,
            label_value,
            min_count,
        } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let matches = session
                .recording()
                .recorded_counters_matching(name, &[(label_key.as_str(), label_value.as_str())]);
            let total: u64 = matches.iter().map(|c| c.delta).sum();
            if total < *min_count {
                return Err(format!(
                    "counter {name} {label_key}={label_value} total {total} < {min_count}"
                ));
            }
        }
        other => return Err(format!("telemetry step mismatch: {other:?}")),
    }
    Ok(())
}
