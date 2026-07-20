//! In-process SurrealDB (`Surreal<Db>`) backend.

use std::any::Any;

use surrealdb::engine::local::Db;
use surrealdb::types::Value as SurrealValueType;
use surrealdb::{Connection, Surreal};

use valence_core::backend::{BackendCapabilities, DatabaseBackend};
use valence_core::compiled_query::CompiledQuery;
use valence_core::error::{Error, Result};
use valence_core::record_id::RecordId;
use valence_core::ttl::{BackendTtlCapability, SchemaTtlPolicy};
use valence_core::KnownEngines;

use crate::error::db_err;
use crate::query_exec::execute_compiled_query_inner;
use crate::record_id::{surreal_from_valence, valence_from_surreal};
use crate::row_json::{
    ensure_schemaless_table, json_to_surreal_content_value, map_looks_like_surreal_thing_only,
    record_map_to_json_object, select_record_json, thing_only_key_from_tb_id_map, thing_to_id_only,
    try_value_as_record_map,
};

/// Stable engine slug for router keys (`surrealdb:logical_name`).
pub const ENGINE_ID: &str = KnownEngines::SURREALDB;

/// Embedded Surreal client type.
pub type SDb = Surreal<Db>;

pub const fn surreal_capabilities() -> BackendCapabilities {
    BackendCapabilities {
        supports_merge: true,
        supports_graph_edges: true,
        telemetry_label: "surrealdb",
    }
}

/// Wraps a local Surreal handle (memory, RocksDB, etc.).
///
/// # Examples
///
/// ```ignore
/// use std::sync::Arc;
/// use valence::{
///     valence_schema, Database, DatabaseFromEngine, FieldType, SDb, SurrealEmbeddedBackend,
///     Valence, SURREAL_ENGINE_ID,
/// };
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", SURREAL_ENGINE_ID);
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
/// let db = SDb::init();
/// db.connect::<surrealdb::engine::local::Mem>(()).await?;
/// db.use_ns("demo").use_db("demo").await?;
/// let valence = Valence::builder()
///     .add_backend("default", Arc::new(SurrealEmbeddedBackend::new(db)))
///     .build()?;
/// assert_eq!(
///     valence.backend_for_table("counter")?.engine_id(),
///     SURREAL_ENGINE_ID
/// );
/// # Ok::<(), valence::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct SurrealEmbeddedBackend {
    db: SDb,
}

impl SurrealEmbeddedBackend {
    /// Wrap an existing embedded Surreal client.
    pub fn new(db: SDb) -> Self {
        Self { db }
    }

    /// Borrow the underlying Surreal client.
    pub fn inner(&self) -> &SDb {
        &self.db
    }

    /// Consume the adapter and return the Surreal client.
    pub fn into_inner(self) -> SDb {
        self.db
    }
}

pub fn strip_id_from_content(mut content: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(ref mut map) = content {
        map.remove("id");
    }
    content
}

pub async fn row_json_after_create<C>(
    db: &Surreal<C>,
    table: &str,
    raw: SurrealValueType,
) -> Result<serde_json::Value>
where
    C: Connection,
{
    let rows: Vec<SurrealValueType> = match raw {
        SurrealValueType::Array(arr) => arr.into_inner(),
        other => vec![other],
    };
    match rows.len() {
        0 => {
            return Err(Error::Validation(
                "Failed to read record after create (empty response)".into(),
            ));
        }
        1 => {}
        _ => {
            return Err(Error::Validation(
                "Unexpected multi-row create response".into(),
            ));
        }
    }
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| Error::Internal("create response length invariant violated".into()))?;
    if let Some(m) = try_value_as_record_map(&row) {
        if map_looks_like_surreal_thing_only(&m) {
            let id = thing_only_key_from_tb_id_map(&m)?;
            return select_record_json(db, table, &id)
                .await?
                .ok_or_else(|| Error::Validation("Failed to read record after create".into()));
        }
        return Ok({
            let mut json = record_map_to_json_object(&m);
            valence_core::row_json::normalize_record_id_field(table, &mut json);
            json
        });
    }

    Err(Error::Validation(
        "Failed to decode create response from database".into(),
    ))
}

#[async_trait::async_trait]
impl DatabaseBackend for SurrealEmbeddedBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        surreal_capabilities()
    }

    fn as_any_local(&self) -> Option<&dyn Any> {
        Some(self as &dyn Any)
    }

    async fn use_namespace(&self, ns: &str, db_name: &str) -> Result<()> {
        self.db.use_ns(ns).use_db(db_name).await.map_err(db_err)?;
        Ok(())
    }

    async fn execute_compiled_query(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        execute_compiled_query_inner(&self.db, &compiled.query_string, &compiled.params).await
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        ensure_schemaless_table(&self.db, table).await?;
        select_record_json(&self.db, table, id).await
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        ensure_schemaless_table(&self.db, table).await?;
        let explicit_id = content
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| thing_to_id_only(s.to_string()))
            .filter(|s| !s.is_empty());
        let json_content = strip_id_from_content(content);
        let resource = match explicit_id.as_deref() {
            Some(id) => surrealdb::opt::Resource::from((table, id)),
            None => surrealdb::opt::Resource::from(table),
        };
        let surreal_content = json_to_surreal_content_value(json_content);
        let raw: SurrealValueType = self
            .db
            .create(resource)
            .content(surreal_content)
            .await
            .map_err(db_err)?;

        if let Some(id_for_get) = explicit_id {
            return select_record_json(&self.db, table, &id_for_get)
                .await?
                .ok_or_else(|| Error::Validation("Failed to read record after create".into()));
        }

        row_json_after_create(&self.db, table, raw).await
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        ensure_schemaless_table(&self.db, table).await?;
        let resource = surrealdb::opt::Resource::from((table, id));
        let content = json_to_surreal_content_value(strip_id_from_content(content));
        let _: SurrealValueType = self
            .db
            .update(resource)
            .content(content)
            .await
            .map_err(db_err)?;
        select_record_json(&self.db, table, id)
            .await?
            .ok_or_else(|| Error::Validation("Failed to read record after update".into()))
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        ensure_schemaless_table(&self.db, table).await?;
        let resource = surrealdb::opt::Resource::from((table, id));
        let patch = json_to_surreal_content_value(strip_id_from_content(patch));
        let _: SurrealValueType = self
            .db
            .update(resource)
            .merge(patch)
            .await
            .map_err(db_err)?;
        select_record_json(&self.db, table, id)
            .await?
            .ok_or_else(|| Error::Validation("Failed to read record after merge".into()))
    }

    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        ensure_schemaless_table(&self.db, table).await?;
        let resource = surrealdb::opt::Resource::from((table, id));
        let content = json_to_surreal_content_value(strip_id_from_content(content));
        let _: SurrealValueType = self
            .db
            .upsert(resource)
            .content(content)
            .await
            .map_err(db_err)?;
        select_record_json(&self.db, table, id)
            .await?
            .ok_or_else(|| Error::Validation("Failed to read record after upsert".into()))
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        let resource = surrealdb::opt::Resource::from((table, id));
        let _: SurrealValueType = self.db.delete(resource).await.map_err(db_err)?;
        Ok(())
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let from_t = surreal_from_valence(from);
        let to_t = surreal_from_valence(to);
        let q = format!("RELATE $from->{edge_table}->$to RETURN NONE");
        ensure_schemaless_table(&self.db, edge_table).await?;
        self.db
            .query(&q)
            .bind(("from", from_t))
            .bind(("to", to_t))
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        let from_t = surreal_from_valence(from);
        let to_t = surreal_from_valence(to);
        let q = format!("DELETE $from->{edge_table} WHERE `out` = $to RETURN NONE");
        ensure_schemaless_table(&self.db, edge_table).await?;
        self.db
            .query(&q)
            .bind(("from", from_t))
            .bind(("to", to_t))
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        use crate::query_exec::query_err_is_missing_table;

        let from_t = surreal_from_valence(from);
        let q = format!("SELECT VALUE `out` FROM {edge_table} WHERE `in` = $from");
        let mut response = match self.db.query(&q).bind(("from", from_t)).await {
            Ok(r) => r,
            Err(e) if query_err_is_missing_table(&e.to_string()) => {
                return Ok(vec![]);
            }
            Err(e) => return Err(db_err(e)),
        };
        let outs: Vec<surrealdb::types::RecordId> = match response.take(0) {
            Ok(r) => r,
            Err(e) if query_err_is_missing_table(&e.to_string()) => {
                return Ok(vec![]);
            }
            Err(e) => return Err(db_err(e)),
        };
        Ok(outs.into_iter().map(valence_from_surreal).collect())
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        ensure_schemaless_table(&self.db, table).await
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        ensure_schemaless_table(&self.db, table).await?;
        let index_name = format!("idx_{table}_{field}_unique");
        let query = format!("DEFINE INDEX {index_name} ON TABLE {table} COLUMNS {field} UNIQUE");
        match self.db.query(&query).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let message = e.to_string().to_lowercase();
                if message.contains("already") && message.contains("index") {
                    Ok(())
                } else {
                    Err(db_err(e))
                }
            }
        }
    }

    fn ttl_capability(&self) -> BackendTtlCapability {
        BackendTtlCapability::Deferred
    }

    async fn apply_ttl_policy(&self, _table: &str, _policy: &SchemaTtlPolicy) -> Result<()> {
        Ok(())
    }
}

/// Alias for [`SurrealEmbeddedBackend`] (historical template name).
pub type SurrealMemBackend = SurrealEmbeddedBackend;

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::print_stdout,
        clippy::print_stderr
    )]

    use super::*;
    use surrealdb::engine::local::Mem;

    async fn mem_backend() -> SurrealEmbeddedBackend {
        let db = SDb::init();
        db.connect::<Mem>(()).await.unwrap();
        db.use_ns("test").use_db("test").await.unwrap();
        SurrealEmbeddedBackend::new(db)
    }

    #[tokio::test]
    async fn create_record_id_field_is_table_object() {
        let b = mem_backend().await;
        let row = b
            .create_record("widget", serde_json::json!({"name": "alpha"}))
            .await
            .expect("create");
        let id = row.get("id").expect("id field");
        assert!(
            id.get("table").is_some() && id.get("id").is_some(),
            "expected RecordId object, got {id:?}"
        );
    }

    #[tokio::test]
    async fn define_unique_index_idempotent() {
        let b = mem_backend().await;
        b.define_unique_index("uniq_tbl", "email")
            .await
            .expect("first define");
        b.define_unique_index("uniq_tbl", "email")
            .await
            .expect("second define idempotent");
    }
}
