//! Example third-party [`DatabaseBackend`] with an open `ENGINE_ID`.
//!
//! Proves custom engines need **no** facade feature — depend on `valence-core`, implement
//! the trait, and wire with `.add_backend(...)`.
//!
//! # Published adapter checklist
//!
//! 1. Depend on `valence-core` only (+ `async-trait`, etc.).
//! 2. `impl DatabaseBackend` + `pub const ENGINE_ID: &str`.
//! 3. Export a schema evaluator (`PRIMARY` below).
//! 4. Optional builder / `from_env_defaults()` for wire engines.
//! 5. Host: `.add_backend("primary", Arc::new(adapter))`.
//!
//! # Examples
//!
//! ```
//! use std::sync::Arc;
//! use acme_valence_backend_stub::{AcmeStubBackend, ENGINE_ID};
//! use valence_core::{DatabaseBackend, Valence};
//!
//! let valence = Valence::builder()
//!     .add_backend("primary", Arc::new(AcmeStubBackend::new()))
//!     .build()
//!     .expect("build");
//! assert_eq!(
//!     valence.active_backend().unwrap().engine_id(),
//!     ENGINE_ID
//! );
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use valence_core::row_json::{record_id_json, thing_to_id_only};
use valence_core::{
    BackendCapabilities, CompiledQuery, Database, DatabaseBackend, DatabaseFromEngine, RecordId,
    Result,
};

/// Open engine slug for router keys (`acme_stub:…`).
pub const ENGINE_ID: &str = "acme_stub";

/// Schema `database:` evaluator for the primary logical name.
pub const PRIMARY: DatabaseFromEngine = Database::from_engine("primary", ENGINE_ID);

/// Minimal in-process stub backend for matrix / contract tests.
#[derive(Debug, Default)]
pub struct AcmeStubBackend {
    store: RwLock<HashMap<String, serde_json::Value>>,
    next_id: AtomicU64,
}

impl AcmeStubBackend {
    /// Empty stub store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for AcmeStubBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_merge: true,
            supports_graph_edges: false,
            telemetry_label: "acme_stub",
        }
    }

    async fn execute_compiled_query(&self, _: &CompiledQuery) -> Result<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        let bare = thing_to_id_only(id.to_string());
        let key = format!("{table}:{bare}");
        let store = self.store.read().await;
        Ok(store.get(&key).cloned())
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = content.get("id").and_then(|v| v.as_str()).map_or_else(
            || format!("acme_{}", self.next_id.fetch_add(1, Ordering::Relaxed)),
            str::to_string,
        );
        let mut out = content;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("id".into(), record_id_json(table, &id));
        } else {
            out = serde_json::json!({ "id": record_id_json(table, &id) });
        }
        let key = format!("{table}:{id}");
        self.store.write().await.insert(key, out.clone());
        Ok(out)
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let key = format!("{table}:{id}");
        self.store.write().await.insert(key, content.clone());
        Ok(content)
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let key = format!("{table}:{id}");
        let mut store = self.store.write().await;
        let mut record = store
            .get(&key)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"id": id}));
        if let Some(obj) = record.as_object_mut() {
            if let Some(patch_obj) = patch.as_object() {
                for (k, v) in patch_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        store.insert(key, record.clone());
        drop(store);
        Ok(record)
    }

    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.update_record(table, id, content).await
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        let key = format!("{table}:{id}");
        self.store.write().await.remove(&key);
        Ok(())
    }

    async fn relate_edge(&self, _: &RecordId, _: &str, _: &RecordId) -> Result<()> {
        Ok(())
    }

    async fn unrelate_edge(&self, _: &RecordId, _: &str, _: &RecordId) -> Result<()> {
        Ok(())
    }

    async fn get_edge_targets(&self, _: &RecordId, _: &str) -> Result<Vec<RecordId>> {
        Ok(vec![])
    }
}
