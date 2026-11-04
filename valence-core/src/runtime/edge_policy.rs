//! Privacy check for ManyToMany edge writes whose connection sets `edge_policy`.

use super::Valence;
use crate::data_use::DataUsePurpose;
use crate::error::{Error, Result};
use crate::privacy::{PrivacyEvaluator, PrivacyOperation};
use crate::query::QueryCore;
use crate::record_id::RecordId;
use crate::schema::{schema_connections_for_table, SchemaMetadata, SchemaRegistry};
use crate::schema_api::SchemaConnection;

/// The `edge_policy` declared for `edge_table` on the schema of `from`, if any.
fn declared_edge_policy(from: &RecordId, edge_table: &str) -> Option<String> {
    let meta = SchemaRegistry::lookup(from.table())?;
    schema_connections_for_table(meta)
        .iter()
        .filter(|c| c.edge_table.as_deref() == Some(edge_table))
        .find_map(|c| c.edge_policy.clone())
}

/// Load the stored row behind `rid`, or `Error::NotFound` when the table or row is missing.
async fn load_row(
    rid: &RecordId,
    valence: &Valence,
    purpose: DataUsePurpose,
) -> Result<(&'static SchemaMetadata, serde_json::Value)> {
    let schema = SchemaRegistry::lookup(rid.table())
        .ok_or_else(|| Error::NotFound(format!("unknown table {}", rid.table())))?;
    let row = QueryCore::get_record_json(rid.table(), rid.id(), valence, purpose)
        .await?
        .ok_or_else(|| Error::NotFound(format!("{}:{} not found", rid.table(), rid.id())))?;
    Ok((schema, row))
}

/// Refuse the edge write unless the connection's `edge_policy` allows it.
///
/// Connections without `edge_policy` pass with no check. For
/// `SourceUpdateTargetRead`, the actor must pass the source schema's Update policy on the
/// stored `from` row and the target schema's Read policy on the stored `to` row.
pub(super) async fn check_edge_write(
    valence: &Valence,
    edge_table: &str,
    from: &RecordId,
    to: &RecordId,
    purpose: DataUsePurpose,
) -> Result<()> {
    let Some(policy) = declared_edge_policy(from, edge_table) else {
        return Ok(());
    };
    if policy != SchemaConnection::EDGE_POLICY_SOURCE_UPDATE_TARGET_READ {
        return Err(Error::Validation(format!(
            "edge {edge_table} on {} has unknown edge_policy {policy:?}",
            from.table()
        )));
    }

    let (from_schema, from_row) = load_row(from, valence, purpose).await?;
    PrivacyEvaluator::check_entity_access(
        from_schema,
        PrivacyOperation::Update,
        &from_row,
        valence,
    )
    .await?;

    let (to_schema, to_row) = load_row(to, valence, purpose).await?;
    PrivacyEvaluator::check_entity_read(to_schema, &to_row, valence).await
}
