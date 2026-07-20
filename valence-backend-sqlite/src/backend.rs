//! SQLite storage engine.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

use valence_backend_sql::{
    create_record_sqlite, define_unique_index_sqlite, delete_record_sqlite, ensure_table_sqlite,
    execute_select_sqlite, get_edge_targets_sqlite, get_record_sqlite, merge_record_sqlite,
    relate_edge_sqlite, sql_capabilities, ttl_deferred, unrelate_edge_sqlite, update_record_sqlite,
};
use valence_core::backend::DatabaseBackend;
use valence_core::compiled_query::CompiledQuery;
use valence_core::error::{Error, Result};
use valence_core::record_id::RecordId;
use valence_core::ttl::SchemaTtlPolicy;
use valence_core::{Database, DatabaseFromEngine, KnownEngines};

/// Stable engine slug for router keys (`sqlite:logical_name`).
pub const ENGINE_ID: &str = KnownEngines::SQLITE;

/// Schema evaluator const for `database:` routing.
pub const PRIMARY: DatabaseFromEngine = Database::from_engine("primary", ENGINE_ID);

/// SQLite-backed [`DatabaseBackend`] using JSON document rows.
///
/// # Examples
///
/// ```ignore
/// use std::sync::Arc;
/// use valence::{
///     valence_schema, Database, DatabaseFromEngine, FieldType, SqliteBackend, Valence,
///     SQLITE_ENGINE_ID,
/// };
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", SQLITE_ENGINE_ID);
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
/// let backend = SqliteBackend::connect_memory().await?;
/// let valence = Valence::builder()
///     .add_backend("default", Arc::new(backend))
///     .build()?;
/// assert_eq!(
///     valence.backend_for_table("counter")?.engine_id(),
///     SQLITE_ENGINE_ID
/// );
/// # Ok::<(), valence::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    /// Connect to an in-memory SQLite database.
    ///
    /// # Errors
    ///
    /// Returns an error if the in-memory connection or edges schema setup fails.
    pub async fn connect_memory() -> Result<Self> {
        Self::connect(":memory:").await
    }

    /// Connect to a SQLite database at `path` (`:memory:` for ephemeral).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Database`] if connecting or ensuring the edges schema fails.
    pub async fn connect(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(path)
            .or_else(|_| SqliteConnectOptions::from_str(&format!("sqlite:{path}")))
            .map_err(|e| Error::Database(e.to_string()))?
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        valence_backend_sql::ensure_edges_sqlite(&pool).await?;
        Ok(Self { pool })
    }

    /// Borrow the underlying pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for SqliteBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> valence_core::BackendCapabilities {
        sql_capabilities("sqlite")
    }

    async fn execute_compiled_query(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        execute_select_sqlite(&self.pool, compiled, "").await
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        ensure_table_sqlite(&self.pool, table).await
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        get_record_sqlite(&self.pool, table, id).await
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        create_record_sqlite(&self.pool, table, content).await
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        update_record_sqlite(&self.pool, table, id, content).await
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        merge_record_sqlite(&self.pool, table, id, patch).await
    }

    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if self.get_record(table, id).await?.is_some() {
            self.update_record(table, id, content).await
        } else {
            let mut c = content;
            if let Some(obj) = c.as_object_mut() {
                obj.insert("id".into(), serde_json::json!({"table": table, "id": id}));
            }
            self.create_record(table, c).await
        }
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        delete_record_sqlite(&self.pool, table, id).await
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        relate_edge_sqlite(&self.pool, from, edge_table, to).await
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        unrelate_edge_sqlite(&self.pool, from, edge_table, to).await
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        get_edge_targets_sqlite(&self.pool, from, edge_table).await
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        define_unique_index_sqlite(&self.pool, table, field).await
    }

    fn ttl_capability(&self) -> valence_core::ttl::BackendTtlCapability {
        ttl_deferred()
    }

    async fn apply_ttl_policy(&self, _table: &str, _policy: &SchemaTtlPolicy) -> Result<()> {
        Ok(())
    }
}
