//! SQLite storage engine.

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePool, SqlitePoolOptions,
};
use sqlx::ConnectOptions;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use valence_backend_sql::{
    apply_ttl_policy_sqlite, create_record_sqlite, define_unique_index_sqlite,
    delete_record_sqlite, ensure_table_sqlite, ensure_typed_table_sqlite, execute_select_sqlite,
    get_edge_sources_sqlite, get_edge_targets_sqlite, get_record_sqlite,
    inspect_typed_layout_sqlite, merge_record_sqlite, read_schema_version_sqlite,
    relate_edge_sqlite, sql_capabilities, sync_typed_table_sqlite, ttl_deferred,
    unrelate_edge_sqlite, update_record_sqlite, write_schema_version_sqlite, WriteEnsureCache,
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

/// SQLite-backed [`DatabaseBackend`] using typed columns from schema layout.
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
    layout_ensured: WriteEnsureCache,
    /// Idle connection outside the pool that keeps an in-memory database alive
    /// across pool reconnects (see [`Self::connect`]).
    _memory_anchor: Option<Arc<Mutex<SqliteConnection>>>,
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
    /// Bare `:memory:` (also `sqlite::memory:` and `sqlite://:memory:`) opens a
    /// shared-cache memory database with a name unique to this backend, so two
    /// backends in one process never see each other's rows.
    ///
    /// In-memory URLs (`:memory:` or `mode=memory`) keep one extra connection open for
    /// the backend's lifetime. SQLite drops a memory database when its last connection
    /// closes, and the pool replaces connections on its own (idle timeout, max lifetime,
    /// or a failed release ping after a cancelled query). Without the extra connection a
    /// replacement would open an empty database while the backend still believed its
    /// tables existed. The data lives until the last clone of the backend is dropped.
    ///
    /// The pool itself stays at one connection for in-memory URLs. Shared-cache mode
    /// uses table locks that the busy timeout does not cover. sqlx waits them out while
    /// stepping a statement, but preparing one while another connection holds the schema
    /// lock (Valence alters tables at write time) fails with `SQLITE_LOCKED`, and so does
    /// a lock cycle between two writers.
    ///
    /// File-backed URLs use WAL and a 5s busy timeout so concurrent schema growth and
    /// writers wait instead of failing immediately with `SQLITE_BUSY`. In-memory URLs
    /// keep the default journal; WAL is not valid for `:memory:`.
    ///
    /// The statement cache is left empty. Mixed-OLTP can add columns while other
    /// connections still run `SELECT *`; a cached column list then panics inside sqlx
    /// when the live row is longer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Database`] if connecting or ensuring the edges schema fails.
    pub async fn connect(path: &str) -> Result<Self> {
        let memory =
            path.contains(":memory:") || path.contains("mode=memory") || path == ":memory:";
        let bare_memory = path
            .trim_start_matches("sqlite://")
            .trim_start_matches("sqlite:")
            == ":memory:";
        let parsed = if bare_memory {
            SqliteConnectOptions::from_str(&format!(
                "file:valence-mem-{}?mode=memory&cache=shared",
                uuid::Uuid::new_v4()
            ))
        } else {
            SqliteConnectOptions::from_str(path)
                .or_else(|_| SqliteConnectOptions::from_str(&format!("sqlite:{path}")))
        };
        let mut options = parsed
            .map_err(|e| Error::database(e.to_string()))?
            .create_if_missing(true)
            .statement_cache_capacity(0)
            .busy_timeout(Duration::from_secs(5));
        if !memory {
            options = options.journal_mode(SqliteJournalMode::Wal);
        }
        let memory_anchor = if memory {
            let conn = options
                .connect()
                .await
                .map_err(|e| Error::database(e.to_string()))?;
            Some(Arc::new(Mutex::new(conn)))
        } else {
            None
        };
        let mut pool_opts = SqlitePoolOptions::new();
        if memory {
            pool_opts = pool_opts.max_connections(1);
        }
        let pool = pool_opts
            .connect_with(options)
            .await
            .map_err(|e| Error::database(e.to_string()))?;
        valence_backend_sql::ensure_edges_sqlite(&pool).await?;
        Ok(Self {
            pool,
            layout_ensured: WriteEnsureCache::new(),
            _memory_anchor: memory_anchor,
        })
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

    async fn inspect_typed_layout(
        &self,
        table: &str,
    ) -> Result<Option<valence_core::storage_layout::StorageLayout>> {
        inspect_typed_layout_sqlite(&self.pool, table).await
    }

    async fn ensure_typed_table(
        &self,
        layout: &valence_core::storage_layout::StorageLayout,
    ) -> Result<()> {
        ensure_typed_table_sqlite(&self.pool, layout).await
    }

    async fn sync_typed_table(
        &self,
        layout: &valence_core::storage_layout::StorageLayout,
    ) -> Result<()> {
        sync_typed_table_sqlite(&self.pool, layout).await
    }

    async fn read_schema_version(&self, table: &str) -> Result<Option<String>> {
        read_schema_version_sqlite(&self.pool, table).await
    }

    async fn write_schema_version(&self, table: &str, version: &str) -> Result<()> {
        write_schema_version_sqlite(&self.pool, table, version).await
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        get_record_sqlite(&self.pool, table, id).await
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        create_record_sqlite(&self.pool, table, content, &self.layout_ensured).await
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        update_record_sqlite(&self.pool, table, id, content, &self.layout_ensured).await
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        merge_record_sqlite(&self.pool, table, id, patch, &self.layout_ensured).await
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

    async fn get_edge_sources(&self, to: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        get_edge_sources_sqlite(&self.pool, to, edge_table).await
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        define_unique_index_sqlite(&self.pool, table, field, &self.layout_ensured).await
    }

    fn ttl_capability(&self) -> valence_core::ttl::BackendTtlCapability {
        ttl_deferred()
    }

    async fn apply_ttl_policy(&self, table: &str, policy: &SchemaTtlPolicy) -> Result<()> {
        apply_ttl_policy_sqlite(&self.pool, table, policy, &self.layout_ensured).await
    }
}
