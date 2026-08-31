//! Defer-to-edge privacy: satellite rows inherit parent access via a named edge.
//!
//! Parent operation mapping:
//! - Read → parent Read
//! - Create → parent **Update** (append / forge-resistant vs open Create)
//! - Update → parent Update
//! - Delete → parent Delete

use std::collections::HashSet;

use crate::actor::Actor;
use crate::connection::extract_id_from_select_value;
use crate::error::{Error, Result};
use crate::query::QueryCore;
use crate::record_id::RecordId;
use crate::runtime::Valence;
use crate::schema::{SchemaMetadata, SchemaRegistry};

use super::PrivacyEvaluator;
use crate::privacy::types::PrivacyOperation;

/// Maximum parent hops for recursive `defer_to_edge` (inclusive of the first hop).
pub const DEFER_TO_EDGE_MAX_DEPTH: u8 = 8;

#[derive(Debug, Default)]
pub(super) struct DeferCtx {
    /// `(table, id)` keys already under evaluation — cycle guard.
    visited: HashSet<(String, String)>,
    depth: u8,
}

/// Parent privacy op evaluated when a satellite op uses `defer_to_edge`.
#[must_use]
pub fn parent_op_for_defer(satellite_op: PrivacyOperation) -> PrivacyOperation {
    match satellite_op {
        PrivacyOperation::Create => PrivacyOperation::Update,
        other => other,
    }
}

impl PrivacyEvaluator {
    /// Resolve `defer_to_edge` from the policy block for `op` (schema, else traits).
    pub(super) fn resolve_defer_to_edge(
        schema: &SchemaMetadata,
        op: PrivacyOperation,
    ) -> Option<String> {
        if let Some(edge) = schema
            .schema
            .policies
            .as_ref()
            .and_then(|p| match op {
                PrivacyOperation::Read => p.read.as_ref(),
                PrivacyOperation::Create => p.create.as_ref(),
                PrivacyOperation::Update => p.update.as_ref(),
                PrivacyOperation::Delete => p.delete.as_ref(),
            })
            .and_then(|r| r.defer_to_edge.clone())
        {
            return Some(edge);
        }

        let trait_reg = crate::TraitRegistry::global();
        for trait_name in &schema.schema.traits {
            let Some(def) = trait_reg.get_definition(trait_name) else {
                continue;
            };
            let Some(policies) = def.policies else {
                continue;
            };
            let rules = match op {
                PrivacyOperation::Read => policies.read,
                PrivacyOperation::Create => policies.create,
                PrivacyOperation::Update => policies.update,
                PrivacyOperation::Delete => policies.delete,
            };
            let Some(rules) = rules else {
                continue;
            };
            if let Some(edge) = rules.defer_to_edge {
                return Some(edge.to_string());
            }
        }
        None
    }

    /// Validate edge name against connections / Record fields; return clear schema error if missing.
    pub(super) fn validate_defer_edge(schema: &SchemaMetadata, edge: &str) -> Result<()> {
        let has_connection = schema
            .schema
            .connections
            .iter()
            .any(|c| c.name == edge || c.from_field == edge)
            || schema
                .schema
                .edges
                .iter()
                .any(|e| e.from_field == edge || e.label.eq_ignore_ascii_case(edge));
        let has_overlay = crate::schema::schema_connections_for_table(schema)
            .iter()
            .any(|c| c.name == edge || c.from_field == edge);
        let has_record_field = schema.schema.fields.iter().any(|f| {
            f.name == edge
                && (f.fk.is_some()
                    || f.field_type.starts_with("Record")
                    || f.field_type.contains("record"))
        });
        let has_trait_connection = {
            let trait_reg = crate::TraitRegistry::global();
            schema.schema.traits.iter().any(|trait_name| {
                trait_reg
                    .get_definition(trait_name)
                    .is_some_and(|def| def.connection_names.contains(&edge))
            })
        };
        if has_connection || has_overlay || has_record_field || has_trait_connection {
            return Ok(());
        }
        Err(Error::Validation(format!(
            "defer_to_edge \"{edge}\" is not a Record/HasOne edge on table {}",
            schema.table_name
        )))
    }

    pub(super) async fn evaluate_defer_to_edge(
        schema: &SchemaMetadata,
        satellite_op: PrivacyOperation,
        raw_data: &serde_json::Value,
        v: &Valence,
        edge: &str,
        ctx: &mut DeferCtx,
        telemetry_label: &str,
    ) -> Result<()> {
        Self::validate_defer_edge(schema, edge)?;

        if ctx.depth >= DEFER_TO_EDGE_MAX_DEPTH {
            let msg =
                format!("Access denied: defer_to_edge depth exceeded ({DEFER_TO_EDGE_MAX_DEPTH})");
            crate::instrumentation::privacy::record_privacy_denial(
                schema.table_name,
                "defer_to_edge_depth",
                telemetry_label,
                &msg,
            );
            return Err(Error::Privacy(msg));
        }

        let Some(edge_val) = raw_data.get(edge).filter(|v| !v.is_null()) else {
            let msg = format!("Access denied: defer_to_edge field \"{edge}\" missing or null");
            crate::instrumentation::privacy::record_privacy_denial(
                schema.table_name,
                "defer_to_edge_missing_source",
                telemetry_label,
                &msg,
            );
            return Err(Error::Privacy(msg));
        };

        let parent = if let Ok(p) = resolve_parent_record_id(edge_val) {
            p
        } else {
            let msg = format!(
                "Access denied: defer_to_edge field \"{edge}\" is not a valid record reference"
            );
            crate::instrumentation::privacy::record_privacy_denial(
                schema.table_name,
                "defer_to_edge_bad_source",
                telemetry_label,
                &msg,
            );
            return Err(Error::Privacy(msg));
        };

        // Cycle key: prefer row id; for Create (often no id yet) use parent ref.
        let row_key = raw_data
            .get("id")
            .and_then(|v| extract_id_from_select_value(v).ok())
            .unwrap_or_else(|| format!("create:{}:{}", parent.table(), parent.id()));
        let key = (schema.table_name.to_string(), row_key);
        if !ctx.visited.insert(key) {
            let msg = "Access denied: defer_to_edge cycle detected".to_string();
            crate::instrumentation::privacy::record_privacy_denial(
                schema.table_name,
                "defer_to_edge_cycle",
                telemetry_label,
                &msg,
            );
            return Err(Error::Privacy(msg));
        }
        ctx.depth = ctx.depth.saturating_add(1);

        let parent_schema = SchemaRegistry::lookup(parent.table()).ok_or_else(|| {
            Error::Validation(format!(
                "defer_to_edge parent schema not found: {}",
                parent.table()
            ))
        })?;

        // Fetch parent as System so missing ACL on storage load does not elevate the viewer.
        let sys = v.with_actor(Actor::System {
            operation: "defer_to_edge_parent_fetch".into(),
        });
        let Some(parent_raw) =
            QueryCore::get_record_json(parent.table(), parent.id(), &sys).await?
        else {
            let msg = format!(
                "Access denied: defer_to_edge parent {}.{} not found",
                parent.table(),
                parent.id()
            );
            crate::instrumentation::privacy::record_privacy_denial(
                schema.table_name,
                "defer_to_edge_missing_parent",
                telemetry_label,
                &msg,
            );
            return Err(Error::Privacy(msg));
        };

        let parent_op = parent_op_for_defer(satellite_op);

        // Recurse with the original viewer actor (not System).
        Box::pin(Self::check_entity_access_with_ctx(
            parent_schema,
            parent_op,
            &parent_raw,
            v,
            ctx,
        ))
        .await
    }
}

fn resolve_parent_record_id(value: &serde_json::Value) -> Result<RecordId> {
    if let Ok(rid) = serde_json::from_value::<RecordId>(value.clone()) {
        return Ok(rid);
    }
    match value {
        serde_json::Value::String(s) => RecordId::parse(s).ok_or_else(|| {
            Error::Validation(format!("invalid RecordId string for defer_to_edge: {s}"))
        }),
        serde_json::Value::Object(map) => {
            let table = map
                .get("table")
                .and_then(|t| t.as_str())
                .ok_or_else(|| Error::Validation("defer_to_edge parent missing table".into()))?;
            let id = map
                .get("id")
                .map(extract_id_from_select_value)
                .transpose()?
                .ok_or_else(|| Error::Validation("defer_to_edge parent missing id".into()))?;
            Ok(RecordId::new(table, id))
        }
        _ => Err(Error::Validation(
            "defer_to_edge parent value must be RecordId or table:id string".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_guard_detects_revisit_unit() {
        let mut ctx = DeferCtx::default();
        assert!(ctx.visited.insert(("hist".into(), "1".into())));
        assert!(!ctx.visited.insert(("hist".into(), "1".into())));
    }

    #[test]
    fn depth_guard_unit() {
        let mut ctx = DeferCtx {
            depth: DEFER_TO_EDGE_MAX_DEPTH,
            ..DeferCtx::default()
        };
        assert!(ctx.depth >= DEFER_TO_EDGE_MAX_DEPTH);
        ctx.depth = 0;
        for _ in 0..DEFER_TO_EDGE_MAX_DEPTH {
            ctx.depth = ctx.depth.saturating_add(1);
        }
        assert_eq!(ctx.depth, DEFER_TO_EDGE_MAX_DEPTH);
    }

    #[test]
    fn resolve_parent_from_string_and_object() {
        let s = resolve_parent_record_id(&serde_json::json!("tag:abc")).unwrap();
        assert_eq!(s.table(), "tag");
        assert_eq!(s.id(), "abc");
        let o =
            resolve_parent_record_id(&serde_json::json!({"table": "tag", "id": "xyz"})).unwrap();
        assert_eq!(o.id(), "xyz");
    }
}
