//! Minimal in-memory storage engine for tests and embedded hosts.

use std::collections::{HashMap, HashSet};
use std::sync::{PoisonError, RwLock as SyncRwLock};

use tokio::sync::{RwLock, RwLockReadGuard};
use valence_core::{BackendCapabilities, CompiledQuery, DatabaseBackend, Error, RecordId, Result};

use crate::index::{candidates_for, EqIndex};

/// Stable engine slug for router keys (`inmemory_mem:logical_name`).
pub const ENGINE_ID: &str = valence_core::KnownEngines::INMEMORY_MEM;

/// In-memory [`DatabaseBackend`] storing rows and graph edges in process memory.
///
/// # Examples
///
/// ```ignore
/// use std::sync::Arc;
/// use valence::{
///     valence_schema, Database, DatabaseFromEngine, FieldType, InMemoryBackend, Valence,
///     MEM_ENGINE_ID,
/// };
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", MEM_ENGINE_ID);
///
/// valence_schema! {
///     Counter {
///         table: "counter",
///         version: "0.1.0",
///         database: COUNTER_DB,
///         fields: [
///             id: { r#type: FieldType::String, primary_key: true, required: true },
///             value: { r#type: FieldType::Integer, required: true },
///         ],
///     }
/// }
///
/// let valence = Valence::builder()
///     .add_backend("default", Arc::new(InMemoryBackend::new()))
///     .build()
///     .expect("build");
/// assert_eq!(
///     valence
///         .backend_for_table("counter")
///         .expect("counter backend")
///         .engine_id(),
///     MEM_ENGINE_ID
/// );
/// ```
#[derive(Debug, Default)]
pub struct InMemoryBackend {
    tables: RwLock<HashMap<String, HashMap<String, serde_json::Value>>>,
    edges: RwLock<HashMap<String, HashSet<(String, String)>>>,
    /// Equality indexes per table, from [`DatabaseBackend::define_index`]. Written only
    /// while `tables` is write-locked, so the two never disagree; never held across an
    /// `.await`.
    indexes: SyncRwLock<HashMap<String, Vec<EqIndex>>>,
}

impl InMemoryBackend {
    /// Create an empty in-memory backend.
    pub fn new() -> Self {
        Self::default()
    }

    async fn table_records_read(
        &self,
        _table: &str,
    ) -> RwLockReadGuard<'_, HashMap<String, HashMap<String, serde_json::Value>>> {
        self.tables.read().await
    }

    /// Rows `compiled` could match: the candidates of an index the query pins, or every
    /// row of `table`. The caller still filters them.
    async fn candidate_rows(
        &self,
        table: &str,
        compiled: &CompiledQuery,
    ) -> Vec<serde_json::Value> {
        let tables = self.table_records_read(table).await;
        let Some(rows) = tables.get(table) else {
            return Vec::new();
        };
        let ids = {
            let indexes = self.indexes.read().unwrap_or_else(PoisonError::into_inner);
            indexes
                .get(table)
                .and_then(|idx| candidates_for(idx, compiled))
        };
        match ids {
            Some(ids) => ids.iter().filter_map(|id| rows.get(id).cloned()).collect(),
            None => rows.values().cloned().collect(),
        }
    }

    /// Move row `id` of `table` from `old` to `new` in every index on that table.
    /// Call with `tables` write-locked.
    fn reindex(
        &self,
        table: &str,
        id: &str,
        old: Option<&serde_json::Value>,
        new: Option<&serde_json::Value>,
    ) {
        let mut indexes = self.indexes.write().unwrap_or_else(PoisonError::into_inner);
        let Some(list) = indexes.get_mut(table) else {
            return;
        };
        for idx in list {
            if let Some(old) = old {
                idx.remove(id, old);
            }
            if let Some(new) = new {
                idx.insert(id, new);
            }
        }
    }

    fn has_indexes(&self, table: &str) -> bool {
        self.indexes
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(table)
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for InMemoryBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::mem()
    }

    async fn execute_compiled_query(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        let q = compiled.query_string.trim();
        let upper = q.to_uppercase();
        if upper.starts_with("RETURN ") && upper.contains("OWNERSHIP_STATUS") {
            let table = compiled_param_str(compiled, "table")
                .ok_or_else(|| Error::Internal("missing table param".into()))?;
            let record_id = compiled_param_str(compiled, "record_id")
                .ok_or_else(|| Error::Internal("missing record_id param".into()))?;
            let ownership_id = compiled_param_str(compiled, "ownership_id")
                .ok_or_else(|| Error::Internal("missing ownership_id param".into()))?;
            let row = self.get_record(&table, &record_id).await?;
            let ownership_status = self
                .get_record("valence_data_ownership", &ownership_id)
                .await?
                .and_then(|r| r.get("status").cloned());
            return Ok(vec![serde_json::json!({
                "row": row,
                "ownership_status": ownership_status,
            })]);
        }

        if upper.starts_with("SELECT ") {
            if upper.contains("COUNT(") {
                if let Some(from_idx) = upper.find(" FROM ") {
                    let table = q[from_idx + 6..]
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !table.is_empty() {
                        let mut rows = self.candidate_rows(table, compiled).await;
                        rows = crate::query_filter::apply_equality_where(rows, compiled);
                        let count = i64::try_from(rows.len()).unwrap_or(i64::MAX);
                        return Ok(vec![serde_json::json!(count)]);
                    }
                }
            }

            if upper.contains("SELECT id") && !upper.contains("body") {
                if let Some(from_idx) = upper.find(" FROM ") {
                    let table = q[from_idx + 6..]
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !table.is_empty() {
                        let rows = {
                            let tables = self.table_records_read(table).await;
                            tables
                                .get(table)
                                .map(|m| {
                                    m.keys()
                                        .map(|id| serde_json::Value::String(id.clone()))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        };
                        return Ok(rows);
                    }
                }
            }

            if upper.contains("body") {
                if let Some(from_idx) = upper.find(" FROM ") {
                    let table = q[from_idx + 6..]
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !table.is_empty() {
                        let mut rows = {
                            let tables = self.table_records_read(table).await;
                            tables
                                .get(table)
                                .map(|m| m.values().cloned().collect::<Vec<_>>())
                                .unwrap_or_default()
                        };
                        if let Some(limit_idx) = upper.rfind(" LIMIT ") {
                            if let Ok(limit) = q[limit_idx + 7..].trim().parse::<usize>() {
                                rows.truncate(limit);
                            }
                        }
                        return Ok(rows);
                    }
                }
            }

            if let Some(from_idx) = upper.find(" FROM ") {
                let table = q[from_idx + 6..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim();
                if !table.is_empty() {
                    let mut rows = self.candidate_rows(table, compiled).await;
                    rows = crate::query_filter::apply_equality_where(rows, compiled);
                    rows =
                        crate::query_filter::apply_order_limit_offset(rows, &compiled.query_string);
                    if upper.contains("SELECT VALUE") || upper.contains("SELECT id") {
                        // Emit bare id cells (string or RecordId object) — not
                        // `{"id": …}` wrappers — so `extract_id_from_select_value` works.
                        return Ok(rows
                            .into_iter()
                            .filter_map(|r| r.get("id").cloned())
                            .collect());
                    }
                    return Ok(rows);
                }
            }
        }
        Ok(vec![])
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        let tables = self.table_records_read(table).await;
        Ok(tables.get(table).and_then(|rows| rows.get(id).cloned()))
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if let Ok(layout) = valence_core::storage_layout::StorageLayout::from_registry_table(table)
        {
            valence_core::storage_layout::validate_write_types(&layout, &content)?;
        }
        let mut content = content;
        valence_core::ttl::prepare_create_content(table, self, &mut content)?;
        let id = storage_id_from_content(&content).unwrap_or_else(uuid_simple);
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            let has_string_id = obj.get("id").and_then(|v| v.as_str()).is_some();
            if !has_string_id {
                obj.insert("id".into(), record_id_json(table, &id));
            }
        }
        let mut tables = self.tables.write().await;
        let old = tables
            .entry(table.to_string())
            .or_default()
            .insert(id.clone(), record.clone());
        self.reindex(table, &id, old.as_ref(), Some(&record));
        drop(tables);
        Ok(record)
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut tables = self.tables.write().await;
        let rows = tables
            .get_mut(table)
            .ok_or_else(|| Error::NotFound(format!("table {table}")))?;
        if !rows.contains_key(id) {
            return Err(Error::NotFound(format!("{table}:{id}")));
        }
        let old = rows.insert(id.to_string(), content.clone());
        self.reindex(table, id, old.as_ref(), Some(&content));
        drop(tables);
        Ok(content)
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut tables = self.tables.write().await;
        let rows = tables.entry(table.to_string()).or_default();
        let existing = rows
            .entry(id.to_string())
            .or_insert_with(|| serde_json::json!({}));
        let before = self.has_indexes(table).then(|| existing.clone());
        if let (Some(base), Some(patch_obj)) = (existing.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                base.insert(k.clone(), v.clone());
            }
        }
        let merged = existing.clone();
        if before.is_some() {
            self.reindex(table, id, before.as_ref(), Some(&merged));
        }
        drop(tables);
        Ok(merged)
    }

    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            obj.insert("id".into(), record_id_json(table, id));
        }
        let mut tables = self.tables.write().await;
        let old = tables
            .entry(table.to_string())
            .or_default()
            .insert(id.to_string(), record.clone());
        self.reindex(table, id, old.as_ref(), Some(&record));
        drop(tables);
        Ok(record)
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        let mut tables = self.tables.write().await;
        if let Some(rows) = tables.get_mut(table) {
            if let Some(old) = rows.remove(id) {
                self.reindex(table, id, Some(&old), None);
            }
        }
        drop(tables);
        Ok(())
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let key = edge_key(edge_table, from);
        self.edges
            .write()
            .await
            .entry(key)
            .or_default()
            .insert((to.table().to_string(), to.id().to_string()));
        Ok(())
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let key = edge_key(edge_table, from);
        if let Some(set) = self.edges.write().await.get_mut(&key) {
            set.remove(&(to.table().to_string(), to.id().to_string()));
        }
        Ok(())
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        let key = edge_key(edge_table, from);
        let edges = self.edges.read().await;
        Ok(edges
            .get(&key)
            .map(|set| {
                set.iter()
                    .map(|(table, id)| RecordId::new(table.clone(), id.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn get_edge_sources(&self, to: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        let edges = self.edges.read().await;
        let prefix = format!("{edge_table}:");
        let mut sources = Vec::new();
        let to_key = (to.table().to_string(), to.id().to_string());
        for (key, set) in edges.iter() {
            if !key.starts_with(&prefix) || !set.contains(&to_key) {
                continue;
            }
            // key = "{edge_table}:{from_table}:{from_id}"
            let rest = &key[prefix.len()..];
            if let Some((ft, fid)) = rest.split_once(':') {
                if !ft.is_empty() && !fid.is_empty() {
                    sources.push(RecordId::new(ft, fid));
                }
            }
        }
        Ok(sources)
    }

    /// No DDL on mem — uniqueness is enforced by the `SELECT VALUE id` probe in codegen.
    async fn define_unique_index(&self, _table: &str, _field: &str) -> Result<()> {
        Err(valence_core::Error::Internal(
            "unique indexes not supported on in-memory backend".into(),
        ))
    }

    /// Mem has no tables to create; this only indexes the layout's `indexed` and `unique`
    /// fields, the ones queries look up by equality. Boot sync calls it through
    /// [`DatabaseBackend::sync_typed_table`].
    async fn ensure_typed_table(
        &self,
        layout: &valence_core::storage_layout::StorageLayout,
    ) -> Result<()> {
        for field in &layout.fields {
            if (field.indexed || field.unique) && !field.primary_key {
                self.define_index(&layout.table, &[field.name.as_str()])
                    .await?;
            }
        }
        Ok(())
    }

    /// Builds the index from the rows already stored, then keeps it current on every write.
    async fn define_index(&self, table: &str, fields: &[&str]) -> Result<()> {
        if fields.is_empty() {
            return Err(Error::Internal(format!(
                "define_index on {table} needs at least one field"
            )));
        }
        // Hold the table lock so no write lands between the backfill and registration.
        let tables = self.tables.write().await;
        let mut indexes = self.indexes.write().unwrap_or_else(PoisonError::into_inner);
        let list = indexes.entry(table.to_string()).or_default();
        if list.iter().any(|idx| idx.covers(fields)) {
            return Ok(());
        }
        let mut idx = EqIndex::new(fields);
        if let Some(rows) = tables.get(table) {
            for (id, row) in rows {
                idx.insert(id, row);
            }
        }
        list.push(idx);
        Ok(())
    }

    fn ttl_capability(&self) -> valence_core::ttl::BackendTtlCapability {
        valence_core::ttl::BackendTtlCapability::Deferred
    }
}

fn edge_key(edge_table: &str, from: &RecordId) -> String {
    format!("{edge_table}:{}:{}", from.table(), from.id())
}

fn compiled_param_str(compiled: &CompiledQuery, key: &str) -> Option<String> {
    compiled
        .params
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
}

fn record_id_json(table: &str, id: &str) -> serde_json::Value {
    serde_json::json!({
        "table": table,
        "id": id,
    })
}

fn storage_id_from_content(content: &serde_json::Value) -> Option<String> {
    let id_val = content.get("id")?;
    if let Some(id) = id_val.get("id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    id_val.as_str().map(|s| s.to_string())
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("mem-{nanos}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[tokio::test]
    async fn crud_round_trip() {
        let backend = InMemoryBackend::new();
        let created = backend
            .create_record("user", serde_json::json!({"name": "Ada"}))
            .await
            .unwrap();
        let id = storage_id_from_content(&created).expect("record id");
        let fetched = backend.get_record("user", &id).await.unwrap().unwrap();
        assert_eq!(fetched["name"], "Ada");

        let merged = backend
            .merge_record("user", &id, serde_json::json!({"name": "Grace"}))
            .await
            .unwrap();
        assert_eq!(merged["name"], "Grace");

        backend.delete_record("user", &id).await.unwrap();
        assert!(backend.get_record("user", &id).await.unwrap().is_none());
    }

    fn pending_query(model: &str, ids: &[&str]) -> CompiledQuery {
        let mut params = vec![
            ("model".to_string(), serde_json::json!(model)),
            ("status".to_string(), serde_json::json!("pending_deletion")),
        ];
        let mut ors = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            ors.push(format!("record_id = $record_id_{i}"));
            params.push((format!("record_id_{i}"), serde_json::json!(id)));
        }
        CompiledQuery::new(
            format!(
                "SELECT record_id FROM valence_data_ownership WHERE valence_model = $model \
                 AND status = $status AND ({})",
                ors.join(" OR ")
            ),
            params,
        )
    }

    async fn sorted_ids(backend: &InMemoryBackend, q: &CompiledQuery) -> Vec<String> {
        let mut out: Vec<String> = backend
            .execute_compiled_query(q)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r["record_id"].as_str().unwrap().to_string())
            .collect();
        out.sort();
        out
    }

    /// Same writes against an indexed and a plain backend must give the same answers,
    /// including after status merges and deletes.
    #[tokio::test]
    async fn define_index_keeps_query_results_happy() {
        let plain = InMemoryBackend::new();
        let indexed = InMemoryBackend::new();
        // Index defined after some rows exist, to cover the backfill.
        for b in [&plain, &indexed] {
            for (id, model, status) in [
                ("o1", "team", "active"),
                ("o2", "team", "pending_deletion"),
                ("o3", "org", "pending_deletion"),
            ] {
                b.upsert_record(
                    "valence_data_ownership",
                    id,
                    serde_json::json!({"valence_model": model, "record_id": id, "status": status}),
                )
                .await
                .unwrap();
            }
        }
        indexed
            .define_index("valence_data_ownership", &["valence_model", "status"])
            .await
            .unwrap();
        indexed
            .define_index("valence_data_ownership", &["valence_model", "status"])
            .await
            .expect("second define is a no-op");

        let all = ["o1", "o2", "o3", "o4"];
        let q = pending_query("team", &all);
        assert_eq!(sorted_ids(&indexed, &q).await, vec!["o2"]);
        assert_eq!(sorted_ids(&plain, &q).await, sorted_ids(&indexed, &q).await);

        for b in [&plain, &indexed] {
            b.merge_record(
                "valence_data_ownership",
                "o1",
                serde_json::json!({"status": "pending_deletion"}),
            )
            .await
            .unwrap();
            b.merge_record(
                "valence_data_ownership",
                "o2",
                serde_json::json!({"status": "deleted"}),
            )
            .await
            .unwrap();
            b.create_record(
                "valence_data_ownership",
                serde_json::json!({"id": "o4", "valence_model": "team", "record_id": "o4", "status": "pending_deletion"}),
            )
            .await
            .unwrap();
        }
        assert_eq!(sorted_ids(&indexed, &q).await, vec!["o1", "o4"]);
        assert_eq!(sorted_ids(&plain, &q).await, sorted_ids(&indexed, &q).await);

        for b in [&plain, &indexed] {
            b.delete_record("valence_data_ownership", "o1")
                .await
                .unwrap();
        }
        assert_eq!(sorted_ids(&indexed, &q).await, vec!["o4"]);
        assert_eq!(sorted_ids(&plain, &q).await, sorted_ids(&indexed, &q).await);

        // A query that doesn't pin every indexed field still sees every row.
        let by_model = CompiledQuery::new(
            "SELECT * FROM valence_data_ownership WHERE valence_model = $model".into(),
            vec![("model".into(), serde_json::json!("team"))],
        );
        assert_eq!(
            indexed
                .execute_compiled_query(&by_model)
                .await
                .unwrap()
                .len(),
            plain.execute_compiled_query(&by_model).await.unwrap().len()
        );
    }

    fn layout_field(
        name: &str,
        primary_key: bool,
        unique: bool,
        indexed: bool,
    ) -> valence_core::storage_layout::LayoutField {
        valence_core::storage_layout::LayoutField {
            name: name.into(),
            storage: valence_core::storage_layout::FieldStorage::String,
            primary_key,
            nullable: !primary_key,
            unique,
            indexed,
            default: None,
            record_table: None,
        }
    }

    #[tokio::test]
    async fn typed_table_indexes_indexed_and_unique_fields_happy() {
        let backend = InMemoryBackend::new();
        let layout = valence_core::storage_layout::StorageLayout {
            table: "permission".into(),
            fields: vec![
                layout_field("id", true, true, false),
                layout_field("name", false, false, true),
                layout_field("email", false, true, false),
                layout_field("note", false, false, false),
            ],
        };
        backend.sync_typed_table(&layout).await.unwrap();
        // Idempotent: a second boot sync must not stack duplicate indexes.
        backend.sync_typed_table(&layout).await.unwrap();
        {
            let indexes = backend.indexes.read().unwrap();
            let list = indexes.get("permission").expect("indexes for permission");
            assert_eq!(list.len(), 2);
            assert!(list.iter().any(|i| i.covers(&["name"])));
            assert!(list.iter().any(|i| i.covers(&["email"])));
        }
        for (id, name) in [("p1", "team.view"), ("p2", "team.edit")] {
            backend
                .upsert_record("permission", id, serde_json::json!({"name": name}))
                .await
                .unwrap();
        }
        let q = CompiledQuery::new(
            "SELECT * FROM permission WHERE name = $n LIMIT 1".into(),
            vec![("n".into(), serde_json::json!("team.edit"))],
        );
        let rows = backend.execute_compiled_query(&q).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "team.edit");
    }

    #[tokio::test]
    async fn define_index_without_fields_is_rejected_sad() {
        let backend = InMemoryBackend::new();
        assert!(backend.define_index("t", &[]).await.is_err());
    }

    #[test]
    fn ttl_capability_is_deferred() {
        let backend = InMemoryBackend::new();
        assert_eq!(
            backend.ttl_capability(),
            valence_core::ttl::BackendTtlCapability::Deferred
        );
    }
}
