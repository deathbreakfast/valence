//! Ownership read paths: lookups, pending-deletion subsets, and owner rollups.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::query::{QueryCore, RecordPredicate, SortDirection};
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

use super::helpers::{
    normalize_pending_deletion_query_value, owner_id_query_values, ownership_colocate_enabled,
    ownership_row_id, parse_count_from_row, schema_skipped_for_owner_summary,
    skip_ownership_for_table, system_valence, OwnerDataSummary, OwnerSchemaRowCount,
    OWNER_SUMMARY_CONCURRENCY,
};
use super::OwnershipService;

impl OwnershipService {
    /// Resolve the backend that stores `valence_data_ownership` rows for `valence_model`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(crate) fn ownership_backend(
        valence_model: &str,
        v: &Valence,
    ) -> Result<Arc<dyn crate::backend::DatabaseBackend>> {
        if ownership_colocate_enabled()
            && !skip_ownership_for_table(valence_model)
            && SchemaRegistry::global().has_schema(valence_model)
        {
            v.backend_for_table(valence_model)
        } else {
            v.backend_for_table("valence_data_ownership")
        }
    }

    async fn pending_deletion_ids_via_in_query(
        valence_model: &str,
        bare_record_ids: &[String],
        v: &Valence,
    ) -> Result<HashSet<String>> {
        let sys = system_valence(v);
        let backend = Self::ownership_backend(valence_model, &sys)?;
        let q = concat!(
            "SELECT VALUE record_id FROM valence_data_ownership ",
            "WHERE valence_model = $model AND status = 'pending_deletion' AND record_id IN $ids"
        );
        let compiled = crate::compiled_query::CompiledQuery::new(
            q.to_string(),
            vec![
                (
                    "model".to_string(),
                    Value::String(valence_model.to_string()),
                ),
                (
                    "ids".to_string(),
                    Value::Array(bare_record_ids.iter().cloned().map(Value::String).collect()),
                ),
            ],
        );
        let rows = backend
            .execute_compiled_query(&compiled)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|v| v.as_str().map(normalize_pending_deletion_query_value))
            .collect())
    }

    /// Load ownership JSON for a row, if present.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn get_ownership_json(
        valence_model: &str,
        record_id: &str,
        v: &Valence,
    ) -> Result<Option<Value>> {
        let id = ownership_row_id(valence_model, record_id);
        let sys = system_valence(v);
        let backend = Self::ownership_backend(valence_model, &sys)?;
        backend
            .get_record("valence_data_ownership", &id)
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    /// Subset of `bare_record_ids` that currently have `status = pending_deletion` in ownership.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn pending_deletion_bare_ids_subset(
        valence_model: &str,
        bare_record_ids: &[String],
        v: &Valence,
    ) -> Result<HashSet<String>> {
        if skip_ownership_for_table(valence_model) || bare_record_ids.is_empty() {
            return Ok(HashSet::new());
        }
        if ownership_colocate_enabled() {
            return Self::pending_deletion_ids_via_in_query(valence_model, bare_record_ids, v)
                .await;
        }

        let sys = system_valence(v);
        const POINT_LOOKUP_MAX: usize = 64;
        if bare_record_ids.len() <= POINT_LOOKUP_MAX {
            let mut out = HashSet::with_capacity(bare_record_ids.len());
            let backend = Self::ownership_backend(valence_model, &sys)?;
            for bare_id in bare_record_ids {
                let id = ownership_row_id(valence_model, bare_id);
                let Some(json) = backend
                    .get_record("valence_data_ownership", &id)
                    .await
                    .map_err(|e| Error::Database(e.to_string()))?
                else {
                    continue;
                };
                if json
                    .get("status")
                    .and_then(|s| s.as_str())
                    .is_some_and(|s| s == "pending_deletion")
                {
                    out.insert(bare_id.clone());
                }
            }
            return Ok(out);
        }

        Self::pending_deletion_ids_via_in_query(valence_model, bare_record_ids, v).await
    }

    async fn count_ownership_rows_for_schema(
        valence_model: &str,
        owner_id: &str,
        owner_type: &str,
        status: &str,
        v: &Valence,
    ) -> Result<u64> {
        let sys = system_valence(v);
        let backend = Self::ownership_backend(valence_model, &sys)?;
        let owner_ids = owner_id_query_values(owner_id, owner_type);
        let q = concat!(
            "SELECT count() AS n FROM valence_data_ownership ",
            "WHERE valence_model = $model AND owner_id IN $owner_ids ",
            "AND owner_type = $owner_type AND status = $status GROUP ALL"
        );
        let compiled = crate::compiled_query::CompiledQuery::new(
            q.to_string(),
            vec![
                (
                    "model".to_string(),
                    Value::String(valence_model.to_string()),
                ),
                (
                    "owner_ids".to_string(),
                    Value::Array(owner_ids.into_iter().map(Value::String).collect()),
                ),
                (
                    "owner_type".to_string(),
                    Value::String(owner_type.to_string()),
                ),
                ("status".to_string(), Value::String(status.to_string())),
            ],
        );
        let rows = backend
            .execute_compiled_query(&compiled)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(rows.first().map_or(0, parse_count_from_row))
    }

    /// Roll up ownership sidecar rows for `owner_id` / `owner_type` across registered schemas.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn owner_data_summary(
        owner_id: &str,
        owner_type: &str,
        v: &Valence,
    ) -> Result<OwnerDataSummary> {
        let registry = SchemaRegistry::global();
        let schemas: Vec<&str> = registry
            .list_schemas()
            .into_iter()
            .filter(|table| {
                registry
                    .get_full_schema(table)
                    .is_some_and(|s| !schema_skipped_for_owner_summary(table, s))
            })
            .collect();

        let semaphore = Arc::new(tokio::sync::Semaphore::new(OWNER_SUMMARY_CONCURRENCY));
        let mut handles = Vec::with_capacity(schemas.len());

        for model in schemas {
            let owner_id = owner_id.to_string();
            let owner_type = owner_type.to_string();
            let v = v.clone();
            let permit = Arc::clone(&semaphore)
                .acquire_owned()
                .await
                .map_err(|e| Error::Internal(e.to_string()))?;
            handles.push(tokio::spawn(async move {
                let _permit = permit;
                let active = Self::count_ownership_rows_for_schema(
                    model,
                    &owner_id,
                    &owner_type,
                    "active",
                    &v,
                )
                .await?;
                let pending = Self::count_ownership_rows_for_schema(
                    model,
                    &owner_id,
                    &owner_type,
                    "pending_deletion",
                    &v,
                )
                .await?;
                Ok::<_, Error>(OwnerSchemaRowCount {
                    valence_model: model.to_string(),
                    active_rows: active,
                    pending_deletion_rows: pending,
                })
            }));
        }

        let mut rows_by_schema = Vec::new();
        for handle in handles {
            let row = handle.await.map_err(|e| Error::Internal(e.to_string()))??;
            if row.active_rows > 0 || row.pending_deletion_rows > 0 {
                rows_by_schema.push(row);
            }
        }

        rows_by_schema.sort_by(|a, b| {
            b.active_rows
                .cmp(&a.active_rows)
                .then_with(|| a.valence_model.cmp(&b.valence_model))
        });

        let owned_rows: u64 = rows_by_schema.iter().map(|r| r.active_rows).sum();
        let pending_deletion_rows: u64 =
            rows_by_schema.iter().map(|r| r.pending_deletion_rows).sum();
        let tables_with_data = rows_by_schema.iter().filter(|r| r.active_rows > 0).count() as u64;

        Ok(OwnerDataSummary {
            owned_rows,
            tables_with_data,
            pending_deletion_rows,
            rows_by_schema,
        })
    }

    /// Recent transfer rows for the ownership row of `valence_model` / `record_id`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn transfer_history(
        valence_model: &str,
        record_id: &str,
        v: &Valence,
        limit: u32,
    ) -> Result<Vec<Value>> {
        let oid = ownership_row_id(valence_model, record_id);
        let sys = system_valence(v);
        let q = QueryCore::new("valence_ownership_transfer".to_string())
            .select(vec!["*".to_string()])
            .where_record(
                "ownership_id".to_string(),
                RecordPredicate::Equals(crate::RecordId::new("valence_data_ownership", &oid)),
            )
            .order_by("transferred_at".to_string(), SortDirection::Desc)
            .limit(limit);
        let rows: Vec<Value> = q.execute(&sys).await?;
        Ok(rows)
    }
}
