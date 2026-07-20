//! Entity-level policy aggregation, trait inheritance, and field filtering.

use crate::error::{Error, Result};
use crate::runtime::Valence;
use crate::schema::SchemaMetadata;
use crate::schema_api::{SchemaField, SchemaPolicyRules};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use super::PrivacyEvaluator;
use crate::privacy::types::{PrivacyOperation, PrivacyPolicy, PrivacyRule};

impl PrivacyEvaluator {
    /// Filter fields from a record based on field-level privacy policies.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub fn filter_entity_fields(
        schema: &SchemaMetadata,
        raw_data: &serde_json::Value,
        viewer: &crate::actor::Actor,
    ) -> Result<(BTreeMap<String, JsonValue>, Vec<String>)> {
        let mut filtered = BTreeMap::new();
        let mut hidden = Vec::new();

        for field in &schema.schema.fields {
            let field_name = field.name.as_str();
            let field_policy = Self::extract_field_read_policy(field)?;

            if let Some(field_value) = raw_data.get(field_name) {
                match Self::evaluate(&field_policy, raw_data, viewer) {
                    Ok(()) => {
                        filtered.insert(field_name.to_string(), field_value.clone());
                    }
                    Err(_) => {
                        hidden.push(field_name.to_string());
                    }
                }
            } else {
                match Self::evaluate(&field_policy, raw_data, viewer) {
                    Ok(()) => {}
                    Err(_) => {
                        hidden.push(field_name.to_string());
                    }
                }
            }
        }

        if filtered.is_empty() && viewer.is_system() {
            if let Some(obj) = raw_data.as_object() {
                for (key, value) in obj {
                    filtered.insert(key.clone(), value.clone());
                }
            }
        }

        let table = schema.table_name;
        crate::instrumentation::privacy::record_field_redactions(table, &hidden);

        Ok((filtered, hidden))
    }

    fn extract_field_read_policy(field_def: &SchemaField) -> Result<PrivacyPolicy> {
        if let Some(policies) = &field_def.policies {
            if let Some(read_policy) = &policies.read {
                return Self::parse_policy_rules(read_policy);
            }
        }
        Ok(PrivacyPolicy::default())
    }

    fn parse_policy_rules(rules: &SchemaPolicyRules) -> Result<PrivacyPolicy> {
        fn sync_rule_from_schema(
            rule: &crate::schema_api::SchemaPolicyRule,
        ) -> Result<PrivacyRule> {
            let privacy_rule = rule.evaluator.ok_or_else(|| {
                Error::Privacy(format!("Policy rule has no evaluator: {}", rule.name))
            })?;
            privacy_rule
                .as_any()
                .downcast_ref::<PrivacyRule>()
                .cloned()
                .ok_or_else(|| {
                    Error::Privacy(format!(
                        "Policy rule cannot run in sync field evaluator: {}",
                        rule.name
                    ))
                })
        }

        let always_allow = rules
            .always_allow
            .iter()
            .map(sync_rule_from_schema)
            .collect::<Result<Vec<_>>>()?;
        let allow = rules
            .allow
            .iter()
            .map(sync_rule_from_schema)
            .collect::<Result<Vec<_>>>()?;
        let block = rules
            .block
            .iter()
            .map(sync_rule_from_schema)
            .collect::<Result<Vec<_>>>()?;
        let always_block = rules
            .always_block
            .iter()
            .map(sync_rule_from_schema)
            .collect::<Result<Vec<_>>>()?;

        Ok(PrivacyPolicy {
            always_allow,
            allow,
            block,
            always_block,
        })
    }

    /// Check entity-level access for any CRUD operation.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn check_entity_access(
        schema: &SchemaMetadata,
        op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &Valence,
    ) -> Result<()> {
        let mut always_block = Vec::<&crate::schema_api::SchemaPolicyRule>::new();
        let mut always_allow = Vec::<&crate::schema_api::SchemaPolicyRule>::new();
        let mut block = Vec::<&crate::schema_api::SchemaPolicyRule>::new();
        let mut allow = Vec::<&crate::schema_api::SchemaPolicyRule>::new();

        Self::append_schema_rules_for_op(
            schema.schema.policies.as_ref(),
            op,
            &mut always_block,
            &mut always_allow,
            &mut block,
            &mut allow,
        );

        Self::append_trait_rules_for_op(
            op,
            &schema.schema.traits,
            &mut always_block,
            &mut always_allow,
            &mut block,
            &mut allow,
        );

        let table = schema.table_name;
        let telemetry_label = v
            .active_backend()
            .map_or("unknown", |b| b.capabilities().telemetry_label);

        if Self::eval_rules_any_match(&always_block, op, raw_data, v, table, "always_block").await?
        {
            let rule_name = always_block
                .first()
                .map_or("always_block", |r| r.name.as_str());
            let msg = format!("Access denied by policy: {rule_name}");
            crate::instrumentation::privacy::record_privacy_denial(
                table,
                rule_name,
                telemetry_label,
                &msg,
            );
            return Err(Error::Privacy(msg));
        }

        if Self::eval_rules_any_match(&always_allow, op, raw_data, v, table, "always_allow").await?
        {
            return Ok(());
        }

        if Self::eval_rules_any_match(&block, op, raw_data, v, table, "block").await? {
            let msg = "Access denied by block policy".to_string();
            crate::instrumentation::privacy::record_privacy_denial(
                table,
                "block",
                telemetry_label,
                &msg,
            );
            return Err(Error::Privacy(msg));
        }

        if Self::eval_rules_any_match(&allow, op, raw_data, v, table, "allow").await? {
            return Ok(());
        }

        if always_allow.is_empty()
            && allow.is_empty()
            && block.is_empty()
            && always_block.is_empty()
        {
            return Ok(());
        }

        let msg = "Access denied: no matching allow policy".to_string();
        crate::instrumentation::privacy::record_privacy_denial(
            table,
            "default_deny",
            telemetry_label,
            &msg,
        );
        Err(Error::Privacy(msg))
    }

    /// Convenience wrapper for the read operation (used by `QueryCore::get_entity`).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn check_entity_read(
        schema: &SchemaMetadata,
        raw_data: &serde_json::Value,
        v: &Valence,
    ) -> Result<()> {
        Self::check_entity_access(schema, PrivacyOperation::Read, raw_data, v).await
    }

    fn append_schema_rules_for_op<'a>(
        policies: Option<&'a crate::schema_api::SchemaPolicies>,
        op: PrivacyOperation,
        always_block: &mut Vec<&'a crate::schema_api::SchemaPolicyRule>,
        always_allow: &mut Vec<&'a crate::schema_api::SchemaPolicyRule>,
        block: &mut Vec<&'a crate::schema_api::SchemaPolicyRule>,
        allow: &mut Vec<&'a crate::schema_api::SchemaPolicyRule>,
    ) {
        let schema_rules = policies.and_then(|p| match op {
            PrivacyOperation::Read => p.read.as_ref(),
            PrivacyOperation::Create => p.create.as_ref(),
            PrivacyOperation::Update => p.update.as_ref(),
            PrivacyOperation::Delete => p.delete.as_ref(),
        });

        let Some(rules) = schema_rules else { return };
        always_block.extend(rules.always_block.iter());
        always_allow.extend(rules.always_allow.iter());
        block.extend(rules.block.iter());
        allow.extend(rules.allow.iter());
    }

    fn append_trait_rules_for_op(
        op: PrivacyOperation,
        traits: &[String],
        always_block: &mut Vec<&crate::schema_api::SchemaPolicyRule>,
        always_allow: &mut Vec<&crate::schema_api::SchemaPolicyRule>,
        block: &mut Vec<&crate::schema_api::SchemaPolicyRule>,
        allow: &mut Vec<&crate::schema_api::SchemaPolicyRule>,
    ) {
        if traits.is_empty() {
            return;
        }
        let trait_reg = crate::TraitRegistry::global();
        for trait_name in traits {
            let Some(def) = trait_reg.get_definition(trait_name) else {
                continue;
            };
            let Some(trait_policies) = def.policies else {
                continue;
            };
            let trait_rules = match op {
                PrivacyOperation::Read => trait_policies.read,
                PrivacyOperation::Create => trait_policies.create,
                PrivacyOperation::Update => trait_policies.update,
                PrivacyOperation::Delete => trait_policies.delete,
            };
            let Some(rules) = trait_rules else { continue };
            always_block.extend(rules.always_block.iter());
            always_allow.extend(rules.always_allow.iter());
            block.extend(rules.block.iter());
            allow.extend(rules.allow.iter());
        }
    }
}
