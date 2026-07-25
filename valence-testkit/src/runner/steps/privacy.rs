//! Privacy and validation steps.

use valence_core::actor::Actor;
use valence_core::privacy::{PrivacyEvaluator, PrivacyOperation};
use valence_core::validation;

use crate::bootstrap::BootstrapSession;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    match step {
        ScenarioStep::AssertPrivacyReadDenied => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let schema = crate::fixtures::authenticated_only_schema();
            let anon = valence.with_actor(Actor::Anonymous);
            let denied =
                PrivacyEvaluator::check_entity_read(schema, &serde_json::json!({"id": "x"}), &anon)
                    .await;
            if denied.is_ok() {
                return Err("anonymous read should be denied".into());
            }
        }
        ScenarioStep::AssertPrivacyWriteDenied => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let schema = crate::fixtures::authenticated_only_schema();
            let anon = valence.with_actor(Actor::Anonymous);
            let denied = PrivacyEvaluator::check_entity_access(
                schema,
                PrivacyOperation::Create,
                &serde_json::json!({"id": "x"}),
                &anon,
            )
            .await;
            if denied.is_ok() {
                return Err("anonymous write should be denied".into());
            }
        }
        ScenarioStep::AssertPrivacyEmptyDefaultDeny => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let schema = crate::fixtures::empty_policies_schema();
            let anon = valence.with_actor(Actor::Anonymous);
            let denied =
                PrivacyEvaluator::check_entity_read(schema, &serde_json::json!({"id": "x"}), &anon)
                    .await;
            if denied.is_ok() {
                return Err("empty policies should default-deny anonymous".into());
            }
            let system = valence.with_actor(Actor::System {
                operation: "testkit".into(),
            });
            PrivacyEvaluator::check_entity_read(schema, &serde_json::json!({"id": "x"}), &system)
                .await
                .map_err(|e| format!("system should pass empty policies: {e}"))?;
        }
        ScenarioStep::AssertValidationRejects { validator, value } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let result = match validator.as_str() {
                "email" => validation::validate_email(value),
                "non_empty" => validation::validate_non_empty(value),
                other => return Err(format!("unsupported validator: {other}")),
            };
            if result.is_ok() {
                return Err(format!(
                    "validator {validator} should reject value {value:?}"
                ));
            }
        }
        ScenarioStep::AssertValidationAccepts { validator, value } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let result = match validator.as_str() {
                "email" => validation::validate_email(value),
                "non_empty" => validation::validate_non_empty(value),
                other => return Err(format!("unsupported validator: {other}")),
            };
            if result.is_err() {
                return Err(format!(
                    "validator {validator} should accept value {value:?}"
                ));
            }
        }
        other => return Err(format!("privacy step mismatch: {other:?}")),
    }
    Ok(())
}
