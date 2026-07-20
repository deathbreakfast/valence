//! IndraDB embedded graph backend using in-memory datastore only.

use std::collections::{HashMap, HashSet};

use indradb::{
    Edge, Identifier, Json, MemoryDatastore, PipePropertyQuery, QueryExt, QueryOutputValue,
    RangeVertexQuery, SpecificVertexQuery, Vertex,
};
use tokio::sync::RwLock;
use uuid::Uuid;
use valence_core::{
    BackendCapabilities, CompiledQuery, Database, DatabaseBackend, DatabaseFromEngine, Error,
    KnownEngines, RecordId, Result,
};

/// Stable engine slug for router keys (`indradb:logical_name`).
pub const ENGINE_ID: &str = KnownEngines::INDRADB;

/// Schema evaluator const for `database:` routing.
pub const PRIMARY: DatabaseFromEngine = Database::from_engine("primary", ENGINE_ID);

const BODY_PROPERTY: &str = "body";

type IndraDb = indradb::Database<MemoryDatastore>;

/// Embedded IndraDB [`DatabaseBackend`] mapping Valence tables to vertex types.
///
/// # Examples
///
/// ```ignore
/// use std::sync::Arc;
/// use valence::{
///     valence_schema, Database, DatabaseFromEngine, FieldType, IndradbBackend, Valence,
///     INDRADB_ENGINE_ID,
/// };
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", INDRADB_ENGINE_ID);
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
///     .add_backend("default", Arc::new(IndradbBackend::new()))
///     .build()?;
/// assert_eq!(
///     valence.backend_for_table("counter")?.engine_id(),
///     INDRADB_ENGINE_ID
/// );
/// # Ok::<(), valence::Error>(())
/// ```
pub struct IndradbBackend {
    db: IndraDb,
    unique_indexes: RwLock<HashMap<(String, String), HashSet<String>>>,
}

impl std::fmt::Debug for IndradbBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndradbBackend")
            .field("unique_indexes", &self.unique_indexes)
            .finish_non_exhaustive()
    }
}

impl Default for IndradbBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl IndradbBackend {
    /// Create an empty in-memory IndraDB backend.
    pub fn new() -> Self {
        Self {
            db: MemoryDatastore::new_db(),
            unique_indexes: RwLock::new(HashMap::new()),
        }
    }

    #[allow(clippy::needless_pass_by_value)] // map_err adapter; value only Display'd
    fn id_err(e: indradb::ValidationError) -> Error {
        Error::Validation(format!("invalid indradb identifier: {e:?}"))
    }

    #[allow(clippy::needless_pass_by_value)] // map_err adapter; value only Display'd
    fn db_err(e: indradb::Error) -> Error {
        Error::Database(e.to_string())
    }

    fn table_identifier(table: &str) -> Result<Identifier> {
        Identifier::new(table).map_err(Self::id_err)
    }

    fn edge_identifier(edge_table: &str) -> Result<Identifier> {
        Identifier::new(edge_table).map_err(Self::id_err)
    }

    fn body_property() -> Result<Identifier> {
        Identifier::new(BODY_PROPERTY).map_err(Self::id_err)
    }

    fn vertex_uuid(table: &str, id: &str) -> Uuid {
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{table}:{id}").as_bytes())
    }

    fn ensure_vertex(&self, table: &str, id: &str) -> Result<Vertex> {
        let vertex_type = Self::table_identifier(table)?;
        let vertex = Vertex::with_id(Self::vertex_uuid(table, id), vertex_type);
        let _ = self.db.create_vertex(&vertex).map_err(Self::db_err)?;
        Ok(vertex)
    }

    fn read_body(&self, vertex_id: Uuid) -> Result<Option<serde_json::Value>> {
        let query = PipePropertyQuery::new(Box::new(SpecificVertexQuery::single(vertex_id).into()))
            .map_err(Self::id_err)?;
        let output = self.db.get(query).map_err(Self::db_err)?;
        for item in output {
            if let QueryOutputValue::VertexProperties(vps) = item {
                for vp in vps {
                    for prop in vp.props {
                        if prop.name.as_str() == BODY_PROPERTY {
                            return Ok(Some(prop.value.0.as_ref().clone()));
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    fn write_body(&self, table: &str, id: &str, body: serde_json::Value) -> Result<()> {
        let vertex = self.ensure_vertex(table, id)?;
        let prop = Self::body_property()?;
        self.db
            .set_properties(
                SpecificVertexQuery::single(vertex.id),
                prop,
                &Json::new(body),
            )
            .map_err(Self::db_err)
    }

    async fn check_unique_fields(
        &self,
        table: &str,
        record: &serde_json::Value,
        exclude_id: Option<&str>,
    ) -> Result<()> {
        let indexes = self.unique_indexes.read().await.clone();
        for ((idx_table, field), values) in indexes.iter() {
            if idx_table != table {
                continue;
            }
            let Some(value) = record.get(field).and_then(|v| v.as_str()) else {
                continue;
            };
            if exclude_id.is_some_and(|id| {
                self.read_body(Self::vertex_uuid(table, id))
                    .ok()
                    .flatten()
                    .and_then(|row| row.get(field).and_then(|v| v.as_str()).map(str::to_string))
                    .is_some_and(|existing| existing == value)
            }) {
                continue;
            }
            if values.contains(value) {
                return Err(Error::Database(format!(
                    "duplicate unique index value for {table}.{field}"
                )));
            }
        }
        Ok(())
    }

    async fn track_unique_fields(&self, table: &str, record: &serde_json::Value) {
        let mut indexes = self.unique_indexes.write().await;
        for ((idx_table, field), values) in indexes.iter_mut() {
            if idx_table != table {
                continue;
            }
            if let Some(value) = record.get(field).and_then(|v| v.as_str()) {
                values.insert(value.to_string());
            }
        }
        drop(indexes);
    }

    async fn untrack_unique_fields(&self, table: &str, record: &serde_json::Value) {
        let mut indexes = self.unique_indexes.write().await;
        for ((idx_table, field), values) in indexes.iter_mut() {
            if idx_table != table {
                continue;
            }
            if let Some(value) = record.get(field).and_then(|v| v.as_str()) {
                values.remove(value);
            }
        }
        drop(indexes);
    }

    fn rows_for_table(&self, table: &str) -> Result<Vec<serde_json::Value>> {
        let vertex_type = Self::table_identifier(table)?;
        let output = self
            .db
            .get(RangeVertexQuery::new().t(vertex_type))
            .map_err(Self::db_err)?;
        let mut rows = Vec::new();
        for item in output {
            if let QueryOutputValue::Vertices(vertices) = item {
                for vertex in vertices {
                    if let Some(body) = self.read_body(vertex.id)? {
                        rows.push(body);
                    }
                }
            }
        }
        Ok(rows)
    }

    fn execute_indra_descriptor(
        &self,
        descriptor: &serde_json::Value,
    ) -> Result<Vec<serde_json::Value>> {
        let table = descriptor
            .get("vertex_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Internal("missing vertex_type in indradb query".into()))?;
        let mut rows = self.rows_for_table(table)?;
        if let Some(limit) = descriptor.get("limit").and_then(|v| v.as_u64()) {
            rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        }
        Ok(rows)
    }

    fn execute_sql_select(&self, q: &str) -> Result<Vec<serde_json::Value>> {
        let upper = q.to_uppercase();
        if !upper.starts_with("SELECT ") {
            return Ok(vec![]);
        }
        let from_idx = upper
            .find(" FROM ")
            .ok_or_else(|| Error::Internal("missing FROM in select".into()))?;
        let table = q[from_idx + 6..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim();
        if table.is_empty() {
            return Ok(vec![]);
        }
        let rows = self.rows_for_table(table)?;
        if upper.contains("SELECT id") && !upper.contains("body") {
            return Ok(rows
                .iter()
                .filter_map(|r| {
                    r.get("id")
                        .and_then(|id| id.get("id").and_then(|x| x.as_str()))
                        .or_else(|| r.get("id").and_then(|id| id.as_str()))
                        .map(|id| serde_json::Value::String(id.to_string()))
                })
                .collect());
        }
        let mut out = rows;
        if let Some(limit_idx) = upper.rfind(" LIMIT ") {
            if let Ok(limit) = q[limit_idx + 7..].trim().parse::<usize>() {
                out.truncate(limit);
            }
        }
        Ok(out)
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for IndradbBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_merge: true,
            supports_graph_edges: true,
            telemetry_label: "indradb",
        }
    }

    async fn execute_compiled_query(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        let q = compiled.query_string.trim();
        if let Ok(descriptor) = serde_json::from_str::<serde_json::Value>(q) {
            if descriptor.get("vertex_type").is_some() {
                return self.execute_indra_descriptor(&descriptor);
            }
        }
        let mut rows = self.execute_sql_select(q)?;
        rows = valence_core::query::apply_equality_where(rows, compiled);
        rows = valence_core::query::apply_order_limit_offset(rows, q);
        Ok(rows)
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        self.read_body(Self::vertex_uuid(table, id))
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.check_unique_fields(table, &content, None).await?;
        let id = storage_id_from_content(&content).unwrap_or_else(uuid_simple);
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            let has_string_id = obj.get("id").and_then(|v| v.as_str()).is_some();
            if !has_string_id {
                obj.insert("id".into(), record_id_json(table, &id));
            }
        }
        self.write_body(table, &id, record.clone())?;
        self.track_unique_fields(table, &record).await;
        Ok(record)
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if self.get_record(table, id).await?.is_none() {
            return Err(Error::NotFound(format!("{table}:{id}")));
        }
        if let Some(existing) = self.get_record(table, id).await? {
            self.untrack_unique_fields(table, &existing).await;
        }
        self.check_unique_fields(table, &content, Some(id)).await?;
        self.write_body(table, id, content.clone())?;
        self.track_unique_fields(table, &content).await;
        Ok(content)
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut record = self
            .get_record(table, id)
            .await?
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(existing) = self.get_record(table, id).await? {
            self.untrack_unique_fields(table, &existing).await;
        }
        if let (Some(base), Some(patch_obj)) = (record.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                base.insert(k.clone(), v.clone());
            }
        }
        self.check_unique_fields(table, &record, Some(id)).await?;
        self.write_body(table, id, record.clone())?;
        self.track_unique_fields(table, &record).await;
        Ok(record)
    }

    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if let Some(existing) = self.get_record(table, id).await? {
            self.untrack_unique_fields(table, &existing).await;
        }
        self.check_unique_fields(table, &content, Some(id)).await?;
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            obj.insert("id".into(), record_id_json(table, id));
        }
        self.write_body(table, id, record.clone())?;
        self.track_unique_fields(table, &record).await;
        Ok(record)
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        if let Some(existing) = self.get_record(table, id).await? {
            self.untrack_unique_fields(table, &existing).await;
        }
        let vertex_id = Self::vertex_uuid(table, id);
        self.db
            .delete(SpecificVertexQuery::single(vertex_id))
            .map_err(Self::db_err)
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        self.ensure_vertex(from.table(), from.id())?;
        self.ensure_vertex(to.table(), to.id())?;
        let edge_type = Self::edge_identifier(edge_table)?;
        let edge = Edge::new(
            Self::vertex_uuid(from.table(), from.id()),
            edge_type,
            Self::vertex_uuid(to.table(), to.id()),
        );
        let _ = self.db.create_edge(&edge).map_err(Self::db_err)?;
        Ok(())
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let edge_type = Self::edge_identifier(edge_table)?;
        let edge = Edge::new(
            Self::vertex_uuid(from.table(), from.id()),
            edge_type,
            Self::vertex_uuid(to.table(), to.id()),
        );
        self.db
            .delete(indradb::SpecificEdgeQuery::single(edge))
            .map_err(Self::db_err)
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        let from_uuid = Self::vertex_uuid(from.table(), from.id());
        let edge_type = Self::edge_identifier(edge_table)?;
        let output = self
            .db
            .get(
                SpecificVertexQuery::single(from_uuid)
                    .outbound()
                    .map_err(Self::id_err)?,
            )
            .map_err(Self::db_err)?;
        let mut targets = Vec::new();
        for item in output {
            if let QueryOutputValue::Edges(edges) = item {
                for edge in edges {
                    if edge.t != edge_type {
                        continue;
                    }
                    let inbound_table = self
                        .db
                        .get(SpecificVertexQuery::single(edge.inbound_id))
                        .map_err(Self::db_err)?
                        .into_iter()
                        .find_map(|value| {
                            if let QueryOutputValue::Vertices(vertices) = value {
                                vertices.first().map(|v| v.t.as_str().to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| from.table().to_string());
                    let body = self
                        .read_body(edge.inbound_id)?
                        .unwrap_or_else(|| serde_json::json!({}));
                    let target_id = storage_id_from_content(&body)
                        .unwrap_or_else(|| edge.inbound_id.to_string());
                    targets.push(RecordId::new(inbound_table, target_id));
                }
            }
        }
        Ok(targets)
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        let needs_populate = {
            let indexes = self.unique_indexes.read().await;
            indexes
                .get(&(table.to_string(), field.to_string()))
                .is_none_or(|entry| entry.is_empty())
        };
        let seeded = if needs_populate {
            let rows = self.rows_for_table(table)?;
            let mut values = HashSet::new();
            for row in rows {
                if let Some(value) = row.get(field).and_then(|v| v.as_str()) {
                    values.insert(value.to_string());
                }
            }
            Some(values)
        } else {
            None
        };
        let mut indexes = self.unique_indexes.write().await;
        let entry = indexes
            .entry((table.to_string(), field.to_string()))
            .or_default();
        if entry.is_empty() {
            if let Some(values) = seeded {
                *entry = values;
            }
        }
        drop(indexes);
        Ok(())
    }
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
    format!("indradb-{nanos}")
}
