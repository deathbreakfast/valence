//! MongoDB wire [`DatabaseBackend`] using the official Rust driver.

use std::collections::HashSet;
use std::sync::Arc;

use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, IndexModel};
use serde_json::{Map, Value};

use valence_core::{
    BackendCapabilities, CompiledQuery, Database, DatabaseBackend, DatabaseFromEngine, Error,
    KnownEngines, RecordId, Result,
};

use crate::config::MongoConfig;

/// Stable engine slug for router keys (`mongodb:logical_name`).
pub const ENGINE_ID: &str = KnownEngines::MONGODB;

/// Schema evaluator const for `database:` routing.
pub const PRIMARY: DatabaseFromEngine = Database::from_engine("primary", ENGINE_ID);

const EDGES_COLLECTION: &str = "valence_edges";

/// MongoDB-backed [`DatabaseBackend`] storing JSON documents per collection.
///
/// # Examples
///
/// ```ignore
/// use std::sync::Arc;
/// use valence::{
///     valence_schema, Database, DatabaseFromEngine, FieldType, MongoBackend, Valence,
///     MONGODB_ENGINE_ID,
/// };
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", MONGODB_ENGINE_ID);
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
/// // Reads VALENCE_MONGODB_URI and optional VALENCE_MONGODB_DATABASE.
/// let backend = MongoBackend::from_env().await?;
/// let valence = Valence::builder()
///     .add_backend("default", Arc::new(backend))
///     .build()?;
/// assert_eq!(
///     valence.backend_for_table("counter")?.engine_id(),
///     MONGODB_ENGINE_ID
/// );
/// # Ok::<(), valence::Error>(())
/// ```
#[derive(Clone)]
pub struct MongoBackend {
    client: Client,
    database: String,
    unique_fields: Arc<tokio::sync::RwLock<HashSet<(String, String)>>>,
}

impl std::fmt::Debug for MongoBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MongoBackend")
            .field("database", &self.database)
            .finish_non_exhaustive()
    }
}

impl MongoBackend {
    /// Start a builder for explicit host wiring.
    pub fn builder() -> crate::config::MongoBackendBuilder {
        crate::config::MongoBackendBuilder::new()
    }

    /// Connect using env defaults via builder (shorthand).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when env config is incomplete, or [`Error::Database`] on connect failure.
    pub async fn from_env() -> Result<Self> {
        Self::builder().from_env_defaults().build().await
    }

    /// Connect to MongoDB at `uri` using database `database`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Database`] when the MongoDB connection fails.
    pub async fn connect(uri: &str, database: &str) -> Result<Self> {
        Self::builder().uri(uri).database(database).build().await
    }

    /// Connect using explicit config.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Database`] when the MongoDB connection fails.
    pub async fn connect_with_config(config: MongoConfig) -> Result<Self> {
        let client = Client::with_uri_str(&config.uri)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        let backend = Self {
            client,
            database: config.database,
            unique_fields: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
        };
        backend.ensure_edges_collection().await?;
        Ok(backend)
    }

    fn assert_safe_table(table: &str) -> Result<()> {
        if table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            Ok(())
        } else {
            Err(Error::Validation(format!("unsafe table name: {table}")))
        }
    }

    fn collection(&self, table: &str) -> Collection<Document> {
        self.client.database(&self.database).collection(table)
    }

    async fn ensure_edges_collection(&self) -> Result<()> {
        let coll = self.collection(EDGES_COLLECTION);
        let index = IndexModel::builder()
            .keys(doc! {
                "from_table": 1,
                "from_id": 1,
                "edge_type": 1,
                "to_table": 1,
                "to_id": 1,
            })
            .options(IndexOptions::builder().unique(true).build())
            .build();
        coll.create_index(index)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn unique_fields_for(&self, table: &str) -> Vec<String> {
        self.unique_fields
            .read()
            .await
            .iter()
            .filter(|(t, _)| t == table)
            .map(|(_, f)| f.clone())
            .collect()
    }

    async fn check_unique_fields(
        &self,
        table: &str,
        record: &Value,
        exclude_id: Option<&str>,
    ) -> Result<()> {
        for field in self.unique_fields_for(table).await {
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
            let coll = self.collection(table);
            let filter = doc! { field.as_str(): value };
            if let Some(existing) = coll
                .find_one(filter)
                .await
                .map_err(|e| Error::Database(e.to_string()))?
            {
                let existing_id = existing.get_str("_id").unwrap_or("");
                if exclude_id != Some(existing_id) {
                    return Err(Error::Database(format!(
                        "duplicate unique index value for {table}.{field}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn value_to_bson(value: &Value) -> mongodb::bson::Bson {
        serde_json::from_value(value.clone()).unwrap_or(mongodb::bson::Bson::Null)
    }

    fn doc_to_row(table: &str, id: &str, doc: Document) -> Value {
        let mut map = Map::new();
        for (k, v) in doc {
            if k == "_id" {
                continue;
            }
            map.insert(k, bson_to_json(v));
        }
        row_from_body(table, id, Value::Object(map))
    }

    async fn rows_for_table(&self, table: &str, limit: Option<usize>) -> Result<Vec<Value>> {
        Self::assert_safe_table(table)?;
        let coll = self.collection(table);
        let mut cursor = coll
            .find(doc! {})
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        let mut rows = Vec::new();
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| Error::Database(e.to_string()))?
        {
            let id = doc.get_str("_id").unwrap_or("").to_string();
            rows.push(Self::doc_to_row(table, &id, doc));
            if limit.is_some_and(|n| rows.len() >= n) {
                break;
            }
        }
        Ok(rows)
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for MongoBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_merge: true,
            supports_graph_edges: true,
            telemetry_label: "mongodb",
        }
    }

    async fn execute_compiled_query(&self, compiled: &CompiledQuery) -> Result<Vec<Value>> {
        let q = compiled.query_string.trim();
        if let Ok(descriptor) = serde_json::from_str::<Value>(q) {
            if let Some(collection) = descriptor.get("collection").and_then(|v| v.as_str()) {
                let limit = descriptor
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| usize::try_from(n).unwrap_or(usize::MAX));
                let mut rows = self.rows_for_table(collection, limit).await?;
                rows = valence_core::query::apply_equality_where(rows, compiled);
                rows = valence_core::query::apply_order_limit_offset(rows, &compiled.query_string);
                return Ok(rows);
            }
        }

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
        // Load all candidates; WHERE / ORDER / LIMIT applied in-process (parity with mem).
        let mut rows = self.rows_for_table(table, None).await?;
        rows = valence_core::query::apply_equality_where(rows, compiled);
        rows = valence_core::query::apply_order_limit_offset(rows, &compiled.query_string);
        if upper.contains("SELECT ID") && !upper.contains("BODY") {
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

    async fn ensure_schemaless_table(&self, _table: &str) -> Result<()> {
        Ok(())
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<Value>> {
        Self::assert_safe_table(table)?;
        let coll = self.collection(table);
        let doc = coll
            .find_one(doc! { "_id": id })
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(doc.map(|d| Self::doc_to_row(table, id, d)))
    }

    async fn create_record(&self, table: &str, content: Value) -> Result<Value> {
        Self::assert_safe_table(table)?;
        self.check_unique_fields(table, &content, None).await?;
        let id = storage_id(&content).unwrap_or_else(uuid_simple);
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            let has_string_id = obj.get("id").and_then(|v| v.as_str()).is_some();
            if !has_string_id {
                obj.insert("id".into(), record_id_json(table, &id));
            }
        }
        let mut doc = body_document(&record);
        doc.insert("_id", id.clone());
        let coll = self.collection(table);
        coll.insert_one(doc).await.map_err(map_duplicate_key)?;
        Ok(record)
    }

    async fn update_record(&self, table: &str, id: &str, content: Value) -> Result<Value> {
        if self.get_record(table, id).await?.is_none() {
            return Err(Error::NotFound(format!("{table}:{id}")));
        }
        self.check_unique_fields(table, &content, Some(id)).await?;
        let mut record = content;
        if let Some(obj) = record.as_object_mut() {
            obj.insert("id".into(), record_id_json(table, id));
        }
        let doc = body_document(&record);
        let coll = self.collection(table);
        coll.replace_one(doc! { "_id": id }, doc)
            .await
            .map_err(map_duplicate_key)?;
        Ok(record)
    }

    async fn merge_record(&self, table: &str, id: &str, patch: Value) -> Result<Value> {
        let existing = self
            .get_record(table, id)
            .await?
            .unwrap_or_else(|| row_from_body(table, id, Value::Object(Map::new())));
        let mut merged = existing;
        if let (Some(base), Some(patch_obj)) = (merged.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                base.insert(k.clone(), v.clone());
            }
        }
        self.check_unique_fields(table, &merged, Some(id)).await?;
        let doc = body_document(&merged);
        let coll = self.collection(table);
        coll.replace_one(doc! { "_id": id }, doc)
            .await
            .map_err(|e| {
                if e.to_string().contains("duplicate key") {
                    Error::Database(format!("duplicate unique index value for {table}"))
                } else {
                    Error::Database(e.to_string())
                }
            })?;
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
        let coll = self.collection(table);
        coll.delete_one(doc! { "_id": id })
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let coll = self.collection(EDGES_COLLECTION);
        coll.insert_one(doc! {
            "from_table": from.table(),
            "from_id": from.id(),
            "edge_type": edge_table,
            "to_table": to.table(),
            "to_id": to.id(),
        })
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let coll = self.collection(EDGES_COLLECTION);
        coll.delete_one(doc! {
            "from_table": from.table(),
            "from_id": from.id(),
            "edge_type": edge_table,
            "to_table": to.table(),
            "to_id": to.id(),
        })
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        let coll = self.collection(EDGES_COLLECTION);
        let mut cursor = coll
            .find(doc! {
                "from_table": from.table(),
                "from_id": from.id(),
                "edge_type": edge_table,
            })
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(edge) = cursor
            .try_next()
            .await
            .map_err(|e| Error::Database(e.to_string()))?
        {
            let to_table = edge.get_str("to_table").unwrap_or("").to_string();
            let to_id = edge.get_str("to_id").unwrap_or("").to_string();
            out.push(RecordId::new(to_table, to_id));
        }
        Ok(out)
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        Self::assert_safe_table(table)?;
        self.unique_fields
            .write()
            .await
            .insert((table.to_string(), field.to_string()));
        let coll = self.collection(table);
        let index = IndexModel::builder()
            .keys(doc! { field: 1 })
            .options(IndexOptions::builder().unique(true).sparse(true).build())
            .build();
        coll.create_index(index)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}

#[allow(clippy::needless_pass_by_value)] // map_err adapter; value only Display'd
fn map_duplicate_key(e: mongodb::error::Error) -> Error {
    if e.to_string().contains("duplicate key") {
        Error::Database("duplicate unique index value".into())
    } else {
        Error::Database(e.to_string())
    }
}

fn body_document(record: &Value) -> Document {
    let mut doc = Document::new();
    if let Some(obj) = record.as_object() {
        for (k, v) in obj {
            if k == "id" {
                continue;
            }
            doc.insert(k.clone(), MongoBackend::value_to_bson(v));
        }
    }
    doc
}

fn bson_to_json(bson: mongodb::bson::Bson) -> Value {
    serde_json::to_value(bson).unwrap_or(Value::Null)
}

fn row_from_body(table: &str, id: &str, body: Value) -> Value {
    let mut obj = match body {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    obj.insert("id".into(), record_id_json(table, id));
    Value::Object(obj)
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
