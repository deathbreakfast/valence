//! Redis wire [`DatabaseBackend`] using JSON documents in Redis STRING keys.

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde_json::{Map, Value};

use valence_core::{
    BackendCapabilities, CompiledQuery, Database, DatabaseBackend, DatabaseFromEngine, Error,
    KnownEngines, RecordId, Result,
};

use crate::config::RedisConfig;
use crate::keys::Keyspace;

/// Stable engine slug for router keys (`redis:logical_name`).
pub const ENGINE_ID: &str = KnownEngines::REDIS;

/// Schema evaluator const for `database:` routing.
pub const PRIMARY: DatabaseFromEngine = Database::from_engine("primary", ENGINE_ID);

/// Redis-backed [`DatabaseBackend`] storing JSON documents per table/id key.
///
/// # Examples
///
/// ```ignore
/// use std::sync::Arc;
/// use valence::{
///     valence_schema, Database, DatabaseFromEngine, FieldType, RedisBackend, Valence,
///     REDIS_ENGINE_ID,
/// };
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", REDIS_ENGINE_ID);
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
/// // Reads VALENCE_REDIS_URL and optional VALENCE_REDIS_KEY_PREFIX.
/// let backend = RedisBackend::from_env().await?;
/// let valence = Valence::builder()
///     .add_backend("default", Arc::new(backend))
///     .build()?;
/// assert_eq!(
///     valence.backend_for_table("counter")?.engine_id(),
///     REDIS_ENGINE_ID
/// );
/// # Ok::<(), valence::Error>(())
/// ```
#[derive(Clone)]
pub struct RedisBackend {
    conn: ConnectionManager,
    keys: Keyspace,
}

impl std::fmt::Debug for RedisBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisBackend")
            .field("keys", &self.keys)
            .finish_non_exhaustive()
    }
}

impl RedisBackend {
    /// Start a builder for explicit host wiring.
    pub fn builder() -> crate::config::RedisBackendBuilder {
        crate::config::RedisBackendBuilder::new()
    }

    /// Connect using env defaults via builder (shorthand).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when env config is incomplete, or [`Error::Database`] on connect failure.
    pub async fn from_env() -> Result<Self> {
        Self::builder().from_env_defaults().build().await
    }

    /// Connect to Redis at `url` with default key prefix.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Database`] when the Redis connection fails.
    pub async fn connect(url: &str) -> Result<Self> {
        Self::builder().url(url).build().await
    }

    /// Connect using explicit config.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Database`] when the Redis connection fails.
    pub async fn connect_with_config(config: RedisConfig) -> Result<Self> {
        let client =
            redis::Client::open(config.url.as_str()).map_err(|e| Error::Database(e.to_string()))?;
        let conn = ConnectionManager::new(client)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(Self {
            conn,
            keys: Keyspace::new(config.key_prefix),
        })
    }

    #[allow(clippy::needless_pass_by_value)] // map_err adapter; value only Display'd
    fn map_err(e: redis::RedisError) -> Error {
        Error::Database(e.to_string())
    }

    fn assert_safe_table(table: &str) -> Result<()> {
        if table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            Ok(())
        } else {
            Err(Error::Validation(format!("unsafe table name: {table}")))
        }
    }

    async fn unique_fields(&self, table: &str) -> Result<Vec<String>> {
        let key = self.keys.uniq_index(table);
        let mut conn = self.conn.clone();
        let fields: Vec<String> = conn.smembers(&key).await.map_err(Self::map_err)?;
        Ok(fields)
    }

    async fn claim_unique_fields(
        &self,
        table: &str,
        id: &str,
        record: &Value,
        exclude_id: Option<&str>,
    ) -> Result<()> {
        for field in self.unique_fields(table).await? {
            let Some(value) = record.get(&field).and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(exclude) = exclude_id {
                if let Ok(Some(row)) = self.get_record(table, exclude).await {
                    if row.get(&field).and_then(|v| v.as_str()) == Some(value) {
                        continue;
                    }
                }
            }
            let key = self.keys.uniq(table, &field, value);
            let mut conn = self.conn.clone();
            let set: bool = conn.set_nx(&key, id).await.map_err(Self::map_err)?;
            if !set {
                let existing: Option<String> = conn.get(&key).await.map_err(Self::map_err)?;
                if existing.as_deref() != Some(id) {
                    return Err(Error::Database(format!(
                        "duplicate unique index value for {table}.{field}"
                    )));
                }
            }
        }
        Ok(())
    }

    async fn release_unique_fields(&self, table: &str, record: &Value) -> Result<()> {
        for field in self.unique_fields(table).await? {
            if let Some(value) = record.get(&field).and_then(|v| v.as_str()) {
                let key = self.keys.uniq(table, &field, value);
                let mut conn = self.conn.clone();
                let _: () = conn.del(&key).await.map_err(Self::map_err)?;
            }
        }
        Ok(())
    }

    async fn rows_for_table(&self, table: &str, limit: Option<usize>) -> Result<Vec<Value>> {
        Self::assert_safe_table(table)?;
        let ids_key = self.keys.table_ids(table);
        let mut conn = self.conn.clone();
        let ids: Vec<String> = conn.smembers(&ids_key).await.map_err(Self::map_err)?;
        let mut rows = Vec::new();
        for id in ids {
            if let Some(row) = self.get_record(table, &id).await? {
                rows.push(row);
            }
            if limit.is_some_and(|n| rows.len() >= n) {
                break;
            }
        }
        Ok(rows)
    }

    fn execute_redis_descriptor(descriptor: &Value) -> Result<(String, Option<usize>)> {
        let index = descriptor
            .get("index")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Internal("missing index in redis query".into()))?;
        let table = index
            .strip_prefix("idx:")
            .ok_or_else(|| Error::Internal(format!("invalid redis index: {index}")))?;
        let limit = descriptor
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX));
        Ok((table.to_string(), limit))
    }

    fn parse_sql_select(q: &str) -> Result<(String, Option<usize>, bool)> {
        let upper = q.to_uppercase();
        if !upper.starts_with("SELECT ") {
            return Err(Error::Internal("not a SELECT query".into()));
        }
        let from_idx = upper
            .find(" FROM ")
            .ok_or_else(|| Error::Internal("missing FROM in select".into()))?;
        let table = q[from_idx + 6..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let id_only = upper.contains("SELECT ID") && !upper.contains("BODY");
        let limit = upper
            .rfind(" LIMIT ")
            .and_then(|idx| q[idx + 7..].trim().parse::<usize>().ok());
        Ok((table, limit, id_only))
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for RedisBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_merge: true,
            supports_graph_edges: true,
            telemetry_label: "redis",
        }
    }

    async fn execute_compiled_query(&self, compiled: &CompiledQuery) -> Result<Vec<Value>> {
        let q = compiled.query_string.trim();
        if let Ok(descriptor) = serde_json::from_str::<Value>(q) {
            if descriptor.get("index").is_some() {
                let (table, _limit) = Self::execute_redis_descriptor(&descriptor)?;
                let mut rows = self.rows_for_table(&table, None).await?;
                rows = valence_core::query::apply_equality_where(rows, compiled);
                rows = valence_core::query::apply_order_limit_offset(rows, &compiled.query_string);
                return Ok(rows);
            }
        }

        let Ok((table, _limit, id_only)) = Self::parse_sql_select(q) else {
            return Ok(vec![]);
        };
        if table.is_empty() {
            return Ok(vec![]);
        }
        // Load all candidates; WHERE / ORDER / LIMIT applied in-process (parity with mem).
        let mut rows = self.rows_for_table(&table, None).await?;
        rows = valence_core::query::apply_equality_where(rows, compiled);
        rows = valence_core::query::apply_order_limit_offset(rows, &compiled.query_string);
        if id_only {
            // Match mem: IdOnlyRecord deserializes `{ "id": ... }`, not bare strings.
            return Ok(rows
                .iter()
                .filter_map(|r| {
                    r.get("id")
                        .and_then(|id| id.get("id").and_then(|x| x.as_str()))
                        .or_else(|| r.get("id").and_then(|id| id.as_str()))
                        .map(|id| serde_json::json!({ "id": id }))
                })
                .collect());
        }
        Ok(rows)
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        Self::assert_safe_table(table)?;
        Ok(())
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<Value>> {
        Self::assert_safe_table(table)?;
        let key = self.keys.doc(table, id);
        let mut conn = self.conn.clone();
        let raw: Option<String> = conn.get(&key).await.map_err(Self::map_err)?;
        Ok(raw.map(|text| {
            let body: Value =
                serde_json::from_str(&text).unwrap_or_else(|_| Value::Object(Map::new()));
            row_from_body(table, id, body)
        }))
    }

    async fn create_record(&self, table: &str, content: Value) -> Result<Value> {
        Self::assert_safe_table(table)?;
        let id = storage_id(&content).unwrap_or_else(uuid_simple);
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            let has_string_id = obj.get("id").and_then(|v| v.as_str()).is_some();
            if !has_string_id {
                obj.insert("id".into(), record_id_json(table, &id));
            }
        }
        self.claim_unique_fields(table, &id, &record, None).await?;
        let body = strip_id_field(&record);
        let body_text =
            serde_json::to_string(&body).map_err(|e| Error::Serialization(e.to_string()))?;
        let doc_key = self.keys.doc(table, &id);
        let ids_key = self.keys.table_ids(table);
        let mut conn = self.conn.clone();
        let _: () = conn
            .set(&doc_key, &body_text)
            .await
            .map_err(Self::map_err)?;
        let _: () = conn.sadd(&ids_key, &id).await.map_err(Self::map_err)?;
        Ok(record)
    }

    async fn update_record(&self, table: &str, id: &str, content: Value) -> Result<Value> {
        let existing = self
            .get_record(table, id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("{table}:{id}")))?;
        self.release_unique_fields(table, &existing).await?;
        self.claim_unique_fields(table, id, &content, Some(id))
            .await?;
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            obj.insert("id".into(), record_id_json(table, id));
        }
        let body = strip_id_field(&record);
        let body_text =
            serde_json::to_string(&body).map_err(|e| Error::Serialization(e.to_string()))?;
        let doc_key = self.keys.doc(table, id);
        let mut conn = self.conn.clone();
        let _: () = conn
            .set(&doc_key, &body_text)
            .await
            .map_err(Self::map_err)?;
        Ok(record)
    }

    async fn merge_record(&self, table: &str, id: &str, patch: Value) -> Result<Value> {
        let existing = self
            .get_record(table, id)
            .await?
            .unwrap_or_else(|| row_from_body(table, id, Value::Object(Map::new())));
        self.release_unique_fields(table, &existing).await?;
        let mut merged = existing;
        if let (Some(base), Some(patch_obj)) = (merged.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                base.insert(k.clone(), v.clone());
            }
        }
        self.claim_unique_fields(table, id, &merged, Some(id))
            .await?;
        let body = strip_id_field(&merged);
        let body_text =
            serde_json::to_string(&body).map_err(|e| Error::Serialization(e.to_string()))?;
        let doc_key = self.keys.doc(table, id);
        let ids_key = self.keys.table_ids(table);
        let mut conn = self.conn.clone();
        let _: () = conn
            .set(&doc_key, &body_text)
            .await
            .map_err(Self::map_err)?;
        let _: () = conn.sadd(&ids_key, id).await.map_err(Self::map_err)?;
        Ok(merged)
    }

    async fn upsert_record(&self, table: &str, id: &str, content: Value) -> Result<Value> {
        if self.get_record(table, id).await?.is_some() {
            self.update_record(table, id, content).await
        } else {
            let mut record = content;
            if let Some(obj) = record.as_object_mut() {
                obj.insert("id".into(), record_id_json(table, id));
            }
            self.create_record(table, record).await
        }
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        if let Some(existing) = self.get_record(table, id).await? {
            self.release_unique_fields(table, &existing).await?;
        }
        let doc_key = self.keys.doc(table, id);
        let ids_key = self.keys.table_ids(table);
        let mut conn = self.conn.clone();
        let _: () = conn.del(&doc_key).await.map_err(Self::map_err)?;
        let _: () = conn.srem(&ids_key, id).await.map_err(Self::map_err)?;
        Ok(())
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let key = self.keys.edge(edge_table, from.table(), from.id());
        let member = format!("{}:{}", to.table(), to.id());
        let mut conn = self.conn.clone();
        let _: () = conn.sadd(&key, member).await.map_err(Self::map_err)?;
        Ok(())
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let key = self.keys.edge(edge_table, from.table(), from.id());
        let member = format!("{}:{}", to.table(), to.id());
        let mut conn = self.conn.clone();
        let _: () = conn.srem(&key, member).await.map_err(Self::map_err)?;
        Ok(())
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        let key = self.keys.edge(edge_table, from.table(), from.id());
        let mut conn = self.conn.clone();
        let members: Vec<String> = conn.smembers(&key).await.map_err(Self::map_err)?;
        Ok(members
            .into_iter()
            .filter_map(|m| {
                let (table, id) = m.split_once(':')?;
                Some(RecordId::new(table.to_string(), id.to_string()))
            })
            .collect())
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        Self::assert_safe_table(table)?;
        let idx_key = self.keys.uniq_index(table);
        let mut conn = self.conn.clone();
        let _: () = conn.sadd(&idx_key, field).await.map_err(Self::map_err)?;
        for row in self.rows_for_table(table, None).await? {
            if let Some(value) = row.get(field).and_then(|v| v.as_str()) {
                let id = row
                    .get("id")
                    .and_then(|v| v.get("id").and_then(|x| x.as_str()))
                    .or_else(|| row.get("id").and_then(|v| v.as_str()))
                    .unwrap_or("");
                if !id.is_empty() {
                    let uniq_key = self.keys.uniq(table, field, value);
                    let _: bool = conn.set_nx(&uniq_key, id).await.map_err(Self::map_err)?;
                }
            }
        }
        Ok(())
    }
}

fn row_from_body(table: &str, id: &str, body: Value) -> Value {
    let mut obj = match body {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    obj.insert("id".into(), record_id_json(table, id));
    Value::Object(obj)
}

fn strip_id_field(record: &Value) -> Map<String, Value> {
    record
        .as_object()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, _)| k != "id")
        .collect()
}

fn record_id_json(table: &str, id: &str) -> Value {
    serde_json::json!({
        "table": table,
        "id": id,
    })
}

fn storage_id(content: &Value) -> Option<String> {
    content.get("id").and_then(|v| {
        v.get("id")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .or_else(|| v.as_str().map(str::to_string))
    })
}

fn uuid_simple() -> String {
    uuid::Uuid::new_v4().to_string()
}
