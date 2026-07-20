//! Synchronous rule evaluation for entity and field policies.

use crate::actor::Actor;
use crate::error::{Error, Result};

use super::PrivacyEvaluator;
use crate::privacy::types::{PrivacyOperation, PrivacyPolicy};

impl PrivacyEvaluator {
    /// Evaluate entity-level privacy policies for any operation.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub fn evaluate(
        policies: &PrivacyPolicy,
        record: &serde_json::Value,
        viewer: &Actor,
    ) -> Result<()> {
        Self::evaluate_policies(policies, record, viewer)
    }

    fn evaluate_policies(
        policies: &PrivacyPolicy,
        record: &serde_json::Value,
        viewer: &Actor,
    ) -> Result<()> {
        for rule in &policies.always_block {
            if (rule.check)(record, viewer) {
                return Err(Error::Privacy(format!(
                    "Access denied by policy: {}",
                    rule.name
                )));
            }
        }

        for rule in &policies.always_allow {
            if (rule.check)(record, viewer) {
                return Ok(());
            }
        }

        for rule in &policies.block {
            if (rule.check)(record, viewer) {
                return Err(Error::Privacy("Access denied by block policy".to_string()));
            }
        }

        for rule in &policies.allow {
            if (rule.check)(record, viewer) {
                return Ok(());
            }
        }

        if policies.always_allow.is_empty()
            && policies.allow.is_empty()
            && policies.block.is_empty()
            && policies.always_block.is_empty()
        {
            return Ok(());
        }

        Err(Error::Privacy(
            "Access denied: no matching allow policy".to_string(),
        ))
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn evaluate_policy_rule(
        rule: &crate::schema_api::SchemaPolicyRule,
        op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &crate::runtime::Valence,
        table: &str,
        rule_phase: &str,
    ) -> Result<bool> {
        let evaluator = rule
            .evaluator
            .ok_or_else(|| Error::Privacy(format!("Unresolved policy rule: {}", rule.name)))?;
        let actor_ctx = crate::ports::actor::JsonActorContext::new(
            serde_json::to_value(v.actor()).unwrap_or(serde_json::Value::Null),
        );
        let matched = evaluator.evaluate(op, raw_data, &actor_ctx, v).await?;
        crate::instrumentation::privacy::record_policy_evaluation(
            &rule.name,
            v.actor(),
            op,
            table,
            rule_phase,
            matched,
            "",
        );
        Ok(matched)
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn eval_rules_any_match(
        rules: &[&crate::schema_api::SchemaPolicyRule],
        op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &crate::runtime::Valence,
        table: &str,
        rule_phase: &str,
    ) -> Result<bool> {
        for rule in rules {
            if Self::evaluate_policy_rule(rule, op, raw_data, v, table, rule_phase).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
