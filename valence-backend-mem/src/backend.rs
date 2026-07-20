//! Minimal in-memory storage engine for tests and embedded hosts.

use std::collections::{HashMap, HashSet};

use tokio::sync::{RwLock, RwLockReadGuard};
use valence_core::{BackendCapabilities, CompiledQuery, DatabaseBackend, Error, RecordId, Result};

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
                        let count = {
                            let tables = self.table_records_read(table).await;
                            tables
                                .get(table)
                                .map_or(0, |m| i64::try_from(m.len()).unwrap_or(i64::MAX))
                        };
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
                    let mut rows = {
                        let tables = self.table_records_read(table).await;
                        tables
                            .get(table)
                            .map(|m| m.values().cloned().collect::<Vec<_>>())
                            .unwrap_or_default()
                    };
                    rows = crate::query_filter::apply_equality_where(rows, compiled);
                    rows =
                        crate::query_filter::apply_order_limit_offset(rows, &compiled.query_string);
                    if upper.contains("SELECT VALUE") || upper.contains("SELECT id") {
                        return Ok(rows
                            .into_iter()
                            .filter_map(|r| {
                                r.get("id").cloned().map(|id| serde_json::json!({"id": id}))
                            })
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
        let id = storage_id_from_content(&content).unwrap_or_else(uuid_simple);
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            let has_string_id = obj.get("id").and_then(|v| v.as_str()).is_some();
            if !has_string_id {
                obj.insert("id".into(), record_id_json(table, &id));
            }
        }
        self.tables
            .write()
            .await
            .entry(table.to_string())
            .or_default()
            .insert(id, record.clone());
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
        rows.insert(id.to_string(), content.clone());
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
        if let (Some(base), Some(patch_obj)) = (existing.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                base.insert(k.clone(), v.clone());
            }
        }
        let merged = existing.clone();
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
        self.tables
            .write()
            .await
            .entry(table.to_string())
            .or_default()
            .insert(id.to_string(), record.clone());
        Ok(record)
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        if let Some(rows) = self.tables.write().await.get_mut(table) {
            rows.remove(id);
        }
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
}
