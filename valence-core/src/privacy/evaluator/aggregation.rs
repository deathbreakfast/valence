//! Entity-level policy aggregation, trait inheritance, and field filtering.

use crate::error::{Error, Result};
use crate::runtime::Valence;
use crate::schema::SchemaMetadata;
use crate::schema_api::{SchemaField, SchemaPolicyRules};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use super::PrivacyEvaluator;
use crate::privacy::types::{PrivacyOperation, PrivacyPolicy, PrivacyRule};

/// Policy name reported when no allow rule matched.
const DEFAULT_DENY: &str = "default_deny";

/// Outcome of an entity access check, keeping the denying policy's name.
enum AccessDecision {
    Allow,
    Deny { policy: String, message: String },
}

impl AccessDecision {
    fn deny_default(message: String) -> Self {
        Self::Deny {
            policy: DEFAULT_DENY.to_string(),
            message,
        }
    }
}

/// Name of the field read rule that denied `viewer`, given that `policy` already denied.
fn field_denial_policy(
    policy: &PrivacyPolicy,
    record: &serde_json::Value,
    viewer: &crate::actor::Actor,
) -> String {
    policy
        .always_block
        .iter()
        .chain(&policy.block)
        .find(|rule| (rule.check)(record, viewer))
        .or_else(|| policy.allow.first())
        .or_else(|| policy.always_allow.first())
        .map_or_else(|| DEFAULT_DENY.to_string(), |rule| rule.name.to_string())
}

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
        let (filtered, hidden, _) =
            Self::filter_entity_fields_with_policies(schema, raw_data, viewer)?;
        Ok((filtered, hidden))
    }

    /// Like [`Self::filter_entity_fields`], and also names the read policy that hid each
    /// hidden field (field name to policy name).
    ///
    /// The name is the field's matching `always_block`/`block` rule, else its first allow
    /// rule (the one the viewer didn't satisfy), else `default_deny`.
    /// # Errors
    ///
    /// Returns an error when a field policy rule cannot run synchronously.
    #[allow(
        clippy::type_complexity,
        reason = "visible fields, hidden field names, and hidden field policies"
    )]
    pub fn filter_entity_fields_with_policies(
        schema: &SchemaMetadata,
        raw_data: &serde_json::Value,
        viewer: &crate::actor::Actor,
    ) -> Result<(
        BTreeMap<String, JsonValue>,
        Vec<String>,
        BTreeMap<String, String>,
    )> {
        let mut filtered = BTreeMap::new();
        let mut hidden = Vec::new();
        let mut hidden_policies = BTreeMap::new();

        for field in &schema.schema.fields {
            let field_name = field.name.as_str();
            // Absent field-level policy means no extra restriction beyond entity-level checks.
            let Some(field_policy) = Self::extract_field_read_policy(field)? else {
                if let Some(field_value) = raw_data.get(field_name) {
                    filtered.insert(field_name.to_string(), field_value.clone());
                }
                continue;
            };

            if Self::evaluate(&field_policy, raw_data, viewer).is_ok() {
                if let Some(field_value) = raw_data.get(field_name) {
                    filtered.insert(field_name.to_string(), field_value.clone());
                }
            } else {
                hidden.push(field_name.to_string());
                hidden_policies.insert(
                    field_name.to_string(),
                    field_denial_policy(&field_policy, raw_data, viewer),
                );
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

        Ok((filtered, hidden, hidden_policies))
    }

    /// Enforce field-level `update` policies on a write to an existing row.
    ///
    /// Generated `update`, `update_with_before` and `upsert` call this before writing.
    ///
    /// A field is checked only when its value in `after` differs from `before`
    /// (a missing field and `null` count as equal, so a field the viewer can't
    /// read never looks changed). `before` of `None` means a create, which this
    /// check leaves to the create policies.
    /// # Errors
    ///
    /// Returns [`Error::Privacy`] naming the first changed field whose update
    /// policy denies `viewer`, or when a policy rule cannot run synchronously.
    pub fn check_field_update_access(
        schema: &SchemaMetadata,
        before: Option<&serde_json::Value>,
        after: &serde_json::Value,
        viewer: &crate::actor::Actor,
    ) -> Result<()> {
        let Some(before) = before else { return Ok(()) };
        if crate::privacy::privacy_bypass_active() {
            return Ok(());
        }
        let norm = |v: Option<&JsonValue>| v.filter(|x| !x.is_null()).cloned();
        for field in &schema.schema.fields {
            let Some(rules) = field.policies.as_ref().and_then(|p| p.update.as_ref()) else {
                continue;
            };
            let name = field.name.as_str();
            if norm(before.get(name)) == norm(after.get(name)) {
                continue;
            }
            let policy = Self::parse_policy_rules(rules)?;
            if Self::evaluate(&policy, after, viewer).is_err() {
                let msg = format!("Field update denied by policy: {name}");
                crate::instrumentation::privacy::record_privacy_denial(
                    schema.table_name,
                    name,
                    "field_update",
                    &msg,
                );
                return Err(Error::Privacy(msg));
            }
        }
        Ok(())
    }

    fn extract_field_read_policy(field_def: &SchemaField) -> Result<Option<PrivacyPolicy>> {
        if let Some(policies) = &field_def.policies {
            if let Some(read_policy) = &policies.read {
                return Ok(Some(Self::parse_policy_rules(read_policy)?));
            }
        }
        Ok(None)
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
    #[tracing::instrument(
        name = "valence.privacy.check_entity_access",
        skip(schema, raw_data, v),
        fields(table = schema.table_name, ?op)
    )]
    pub async fn check_entity_access(
        schema: &SchemaMetadata,
        op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &Valence,
    ) -> Result<()> {
        let mut ctx = super::defer_to_edge::DeferCtx::default();
        Self::check_entity_access_with_ctx(schema, op, raw_data, v, &mut ctx).await
    }

    pub(super) async fn check_entity_access_with_ctx(
        schema: &SchemaMetadata,
        op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &Valence,
        ctx: &mut super::defer_to_edge::DeferCtx,
    ) -> Result<()> {
        match Self::decide_entity_access_with_ctx(schema, op, raw_data, v, ctx).await? {
            AccessDecision::Allow => Ok(()),
            AccessDecision::Deny { message, .. } => Err(Error::Privacy(message)),
        }
    }

    /// Entity Read check that names the policy which denied, for
    /// [`crate::query::QueryCore::get_entity_read`].
    ///
    /// Returns `Ok(None)` when the actor may read the row and `Ok(Some(policy))` when a
    /// policy denies it. `policy` is the matching `always_block`/`block` rule's name,
    /// `default_deny` when no allow rule matched, or `defer_to_edge:<edge>` when the
    /// denial came from the parent the read defers to.
    /// # Errors
    ///
    /// Policy evaluation errors other than a denial.
    pub async fn entity_read_denial(
        schema: &SchemaMetadata,
        raw_data: &serde_json::Value,
        v: &Valence,
    ) -> Result<Option<String>> {
        let mut ctx = super::defer_to_edge::DeferCtx::default();
        match Self::decide_entity_access_with_ctx(
            schema,
            PrivacyOperation::Read,
            raw_data,
            v,
            &mut ctx,
        )
        .await?
        {
            AccessDecision::Allow => Ok(None),
            AccessDecision::Deny { policy, .. } => Ok(Some(policy)),
        }
    }

    /// First rule in `rules` that matches, short-circuiting like `eval_rules_any_match`.
    async fn first_matching_rule<'a>(
        rules: &[&'a crate::schema_api::SchemaPolicyRule],
        op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &Valence,
        table: &str,
        rule_phase: &str,
    ) -> Result<Option<&'a crate::schema_api::SchemaPolicyRule>> {
        for rule in rules {
            if Self::evaluate_policy_rule(rule, op, raw_data, v, table, rule_phase).await? {
                return Ok(Some(rule));
            }
        }
        Ok(None)
    }

    async fn decide_entity_access_with_ctx(
        schema: &SchemaMetadata,
        op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &Valence,
        ctx: &mut super::defer_to_edge::DeferCtx,
    ) -> Result<AccessDecision> {
        // Bench/test seam only — never enable in production hosts.
        if crate::privacy::privacy_bypass_active() {
            return Ok(AccessDecision::Allow);
        }

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

        if let Some(matched) =
            Self::first_matching_rule(&always_block, op, raw_data, v, table, "always_block").await?
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
            return Ok(AccessDecision::Deny {
                policy: matched.name.clone(),
                message: msg,
            });
        }

        if Self::eval_rules_any_match(&always_allow, op, raw_data, v, table, "always_allow").await?
        {
            return Ok(AccessDecision::Allow);
        }

        if let Some(matched) =
            Self::first_matching_rule(&block, op, raw_data, v, table, "block").await?
        {
            let msg = "Access denied by block policy".to_string();
            crate::instrumentation::privacy::record_privacy_denial(
                table,
                "block",
                telemetry_label,
                &msg,
            );
            return Ok(AccessDecision::Deny {
                policy: matched.name.clone(),
                message: msg,
            });
        }

        if Self::eval_rules_any_match(&allow, op, raw_data, v, table, "allow").await? {
            return Ok(AccessDecision::Allow);
        }

        if let Some(ref edge) = Self::resolve_defer_to_edge(schema, op) {
            return match Self::evaluate_defer_to_edge(
                schema,
                op,
                raw_data,
                v,
                edge,
                ctx,
                telemetry_label,
            )
            .await
            {
                Ok(()) => Ok(AccessDecision::Allow),
                Err(Error::Privacy(message)) => Ok(AccessDecision::Deny {
                    policy: format!("defer_to_edge:{edge}"),
                    message,
                }),
                Err(e) => Err(e),
            };
        }

        if always_allow.is_empty()
            && allow.is_empty()
            && block.is_empty()
            && always_block.is_empty()
        {
            // Default-deny for missing entity policies. System retains access so platform/bootstrap
            // paths on unpolicied internal tables keep working; declare explicit policies for data.
            if v.actor().is_system() {
                return Ok(AccessDecision::Allow);
            }
            let msg = "Access denied: no privacy policies configured (default deny)".to_string();
            crate::instrumentation::privacy::record_privacy_denial(
                table,
                DEFAULT_DENY,
                telemetry_label,
                &msg,
            );
            return Ok(AccessDecision::deny_default(msg));
        }

        let msg = "Access denied: no matching allow policy".to_string();
        crate::instrumentation::privacy::record_privacy_denial(
            table,
            DEFAULT_DENY,
            telemetry_label,
            &msg,
        );
        Ok(AccessDecision::deny_default(msg))
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
