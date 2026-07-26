//! Privacy and validation steps.

use valence_core::actor::Actor;
use valence_core::privacy::{
    privacy_bypass_active, PrivacyEvaluator, PrivacyOperation, PRIVACY_BYPASS_ENV,
    PRIVACY_BYPASS_FORCE_ON_ENV,
};
use valence_core::query::QueryCore;
use valence_core::validation;
use valence_core::MAX_QUERY_LIMIT;

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
        ScenarioStep::AssertPrivacyFieldSystemOnlyHidden => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let schema = crate::fixtures::system_only_field_schema();
            let raw = serde_json::json!({"id": "1", "secret": "hidden-value"});
            let (filtered, hidden) =
                PrivacyEvaluator::filter_entity_fields(schema, &raw, &Actor::Anonymous)
                    .map_err(|e| e.to_string())?;
            if filtered.contains_key("secret") {
                return Err("SYSTEM_ONLY secret should be hidden from anonymous".into());
            }
            if !hidden.iter().any(|f| f == "secret") {
                return Err("secret should appear in hidden fields".into());
            }
            let (sys_filtered, _) = PrivacyEvaluator::filter_entity_fields(
                schema,
                &raw,
                &Actor::System {
                    operation: "testkit".into(),
                },
            )
            .map_err(|e| e.to_string())?;
            if !sys_filtered.contains_key("secret") {
                return Err("SYSTEM_ONLY secret should be visible to System".into());
            }
        }
        ScenarioStep::AssertQueryLimitClamped => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let q = QueryCore::new("project".into()).limit(u32::MAX);
            if q.limit != Some(MAX_QUERY_LIMIT) {
                return Err(format!(
                    "expected limit clamped to {MAX_QUERY_LIMIT}, got {:?}",
                    q.limit
                ));
            }
        }
        ScenarioStep::AssertPrivacyBypassRequiresForce => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            std::env::set_var(PRIVACY_BYPASS_ENV, "1");
            std::env::remove_var(PRIVACY_BYPASS_FORCE_ON_ENV);
            if privacy_bypass_active() {
                std::env::remove_var(PRIVACY_BYPASS_ENV);
                return Err("bypass alone must not activate without FORCE_ON".into());
            }
            std::env::set_var(PRIVACY_BYPASS_FORCE_ON_ENV, "1");
            if !privacy_bypass_active() {
                std::env::remove_var(PRIVACY_BYPASS_ENV);
                std::env::remove_var(PRIVACY_BYPASS_FORCE_ON_ENV);
                return Err("bypass+FORCE_ON should activate".into());
            }
            std::env::remove_var(PRIVACY_BYPASS_ENV);
            std::env::remove_var(PRIVACY_BYPASS_FORCE_ON_ENV);
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
