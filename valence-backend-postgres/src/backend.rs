//! Postgres storage engine.

use sqlx::postgres::PgPool;

use valence_backend_sql::{
    create_record_postgres, define_unique_index_postgres, delete_record_postgres,
    ensure_edges_postgres, ensure_table_postgres, execute_select_postgres,
    get_edge_targets_postgres, get_record_postgres, merge_record_postgres, relate_edge_postgres,
    sql_capabilities, ttl_deferred, unrelate_edge_postgres, update_record_postgres,
};
use valence_core::backend::DatabaseBackend;
use valence_core::compiled_query::CompiledQuery;
use valence_core::error::{Error, Result};
use valence_core::record_id::RecordId;
use valence_core::ttl::SchemaTtlPolicy;
use valence_core::{Database, DatabaseFromEngine, KnownEngines};

/// Stable engine slug for router keys (`postgres:logical_name`).
pub const ENGINE_ID: &str = KnownEngines::POSTGRES;

/// Schema evaluator const for `database:` routing.
pub const PRIMARY: DatabaseFromEngine = Database::from_engine("primary", ENGINE_ID);

/// Postgres-backed [`DatabaseBackend`] using JSONB document rows.
///
/// # Examples
///
/// ```ignore
/// use std::sync::Arc;
/// use valence::{
///     valence_schema, Database, DatabaseFromEngine, FieldType, PostgresBackend, Valence,
///     POSTGRES_ENGINE_ID,
/// };
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", POSTGRES_ENGINE_ID);
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
/// // Reads DATABASE_URL.
/// let backend = PostgresBackend::from_env().await?;
/// let valence = Valence::builder()
///     .add_backend("default", Arc::new(backend))
///     .build()?;
/// assert_eq!(
///     valence.backend_for_table("counter")?.engine_id(),
///     POSTGRES_ENGINE_ID
/// );
/// # Ok::<(), valence::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct PostgresBackend {
    pool: PgPool,
}

impl PostgresBackend {
    /// Start a builder for explicit host wiring.
    pub fn builder() -> crate::config::PostgresBackendBuilder {
        crate::config::PostgresBackendBuilder::new()
    }

    /// Connect using env defaults via builder (shorthand).
    ///
    /// # Errors
    ///
    /// Returns an error if `DATABASE_URL` is missing or the connection fails.
    pub async fn from_env() -> Result<Self> {
        Self::builder().from_env_defaults().build().await
    }

    /// Connect using a Postgres connection URL.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Database`] if connecting or ensuring the edges schema fails.
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        ensure_edges_postgres(&pool).await?;
        Ok(Self { pool })
    }

    /// Borrow the underlying pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for PostgresBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> valence_core::BackendCapabilities {
        sql_capabilities("postgres")
    }

    async fn execute_compiled_query(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        execute_select_postgres(&self.pool, compiled, "").await
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        ensure_table_postgres(&self.pool, table).await
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        get_record_postgres(&self.pool, table, id).await
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        create_record_postgres(&self.pool, table, content).await
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        update_record_postgres(&self.pool, table, id, content).await
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        merge_record_postgres(&self.pool, table, id, patch).await
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
        delete_record_postgres(&self.pool, table, id).await
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        relate_edge_postgres(&self.pool, from, edge_table, to).await
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        unrelate_edge_postgres(&self.pool, from, edge_table, to).await
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        get_edge_targets_postgres(&self.pool, from, edge_table).await
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        define_unique_index_postgres(&self.pool, table, field).await
    }

    fn ttl_capability(&self) -> valence_core::ttl::BackendTtlCapability {
        ttl_deferred()
    }

    async fn apply_ttl_policy(&self, _table: &str, _policy: &SchemaTtlPolicy) -> Result<()> {
        Ok(())
    }
}
