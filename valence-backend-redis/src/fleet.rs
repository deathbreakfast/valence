//! Multi-node Redis fleet routing for bench campaigns.

use std::sync::Arc;

use valence_core::backend::DatabaseBackend;
use valence_core::Result;

use crate::backend::RedisBackend;
use crate::config::{FleetRedisBackendBuilder, RedisConfig};

/// Routes table operations across standalone Redis nodes by table hash.
#[derive(Debug)]
pub struct FleetRedisBackend {
    backends: Vec<RedisBackend>,
}

impl FleetRedisBackend {
    /// Start a builder for explicit fleet wiring.
    pub fn builder() -> FleetRedisBackendBuilder {
        FleetRedisBackendBuilder::new()
    }

    /// Connect using env defaults via builder (shorthand).
    ///
    /// # Errors
    ///
    /// Returns [`valence_core::Error::Internal`] when env config is incomplete, or
    /// [`valence_core::Error::Database`] on connect failure.
    pub async fn from_env() -> Result<Self> {
        Self::builder().from_env_defaults().build().await
    }

    /// Connect to every URL with shared key prefix.
    ///
    /// # Errors
    ///
    /// Returns [`valence_core::Error::Database`] when any node connection fails.
    pub async fn connect_with_urls(urls: Vec<String>, key_prefix: String) -> Result<Self> {
        let mut backends = Vec::with_capacity(urls.len());
        for url in urls {
            backends.push(
                RedisBackend::connect_with_config(RedisConfig::new(url, key_prefix.clone()))
                    .await?,
            );
        }
        Ok(Self { backends })
    }

    fn backend_for_table(&self, table: &str) -> &RedisBackend {
        let idx = table_slot_index(table, self.backends.len());
        &self.backends[idx]
    }
}

fn table_slot_index(table: &str, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    table
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(usize::from(b)))
        % n
}

#[async_trait::async_trait]
impl DatabaseBackend for FleetRedisBackend {
    fn engine_id(&self) -> &'static str {
        crate::ENGINE_ID
    }

    fn capabilities(&self) -> valence_core::BackendCapabilities {
        self.backends[0].capabilities()
    }

    async fn execute_compiled_query(
        &self,
        compiled: &valence_core::CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        self.backends[0].execute_compiled_query(compiled).await
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        for backend in &self.backends {
            backend.ensure_schemaless_table(table).await?;
        }
        Ok(())
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        self.backend_for_table(table).get_record(table, id).await
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.backend_for_table(table)
            .create_record(table, content)
            .await
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.backend_for_table(table)
            .update_record(table, id, content)
            .await
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.backend_for_table(table)
            .merge_record(table, id, patch)
            .await
    }

    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.backend_for_table(table)
            .upsert_record(table, id, content)
            .await
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        self.backend_for_table(table).delete_record(table, id).await
    }

    async fn relate_edge(
        &self,
        from: &valence_core::RecordId,
        edge_table: &str,
        to: &valence_core::RecordId,
    ) -> Result<()> {
        self.backend_for_table(from.table())
            .relate_edge(from, edge_table, to)
            .await
    }

    async fn unrelate_edge(
        &self,
        from: &valence_core::RecordId,
        edge_table: &str,
        to: &valence_core::RecordId,
    ) -> Result<()> {
        self.backend_for_table(from.table())
            .unrelate_edge(from, edge_table, to)
            .await
    }

    async fn get_edge_targets(
        &self,
        from: &valence_core::RecordId,
        edge_table: &str,
    ) -> Result<Vec<valence_core::RecordId>> {
        self.backend_for_table(from.table())
            .get_edge_targets(from, edge_table)
            .await
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        for backend in &self.backends {
            backend.define_unique_index(table, field).await?;
        }
        Ok(())
    }
}

/// Install a fleet backend as `Arc<dyn DatabaseBackend>`.
///
/// # Errors
///
/// Propagates builder/`build` errors (missing config or connection failure).
pub async fn connect_fleet_arc(
    builder: FleetRedisBackendBuilder,
) -> Result<Arc<dyn DatabaseBackend>> {
    Ok(Arc::new(builder.build().await?))
}
