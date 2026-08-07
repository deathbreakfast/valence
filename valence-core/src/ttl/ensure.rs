//! Ensure TTL policies for one table or all registered TTL schemas.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::backend::DatabaseBackend;
use crate::error::Result;
use crate::schema::SchemaRegistry;
use crate::Valence;

use super::policy::BackendTtlCapability;
use super::stamp::policy_for_table;

static WARNED_TABLES: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static WARN_EMIT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Reset once-per-table TTL warn state (matrix / integration harnesses only).
#[doc(hidden)]
pub fn reset_ttl_warn_state_for_tests() {
    let mut guard = WARNED_TABLES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = None;
    WARN_EMIT_COUNT.store(0, Ordering::SeqCst);
}

/// Number of non-native TTL warnings emitted since the last reset (harness only).
#[doc(hidden)]
#[must_use]
pub fn ttl_warn_emit_count_for_tests() -> usize {
    WARN_EMIT_COUNT.load(Ordering::SeqCst)
}

fn warn_non_native_once(table: &str, engine_id: &str, capability: BackendTtlCapability) {
    let mut guard = WARNED_TABLES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let set = guard.get_or_insert_with(HashSet::new);
    if !set.insert(table.to_string()) {
        return;
    }
    let capability_label = match capability {
        BackendTtlCapability::Deferred => "deferred",
        BackendTtlCapability::Unsupported => "unsupported",
        BackendTtlCapability::SupportedNative => "native",
    };
    WARN_EMIT_COUNT.fetch_add(1, Ordering::SeqCst);
    tracing::warn!(
        target: "valence_ttl",
        table,
        engine_id,
        capability = capability_label,
        "schema TTL is not natively supported on this backend; wire valence_platform::ttl_sweep (register_ttl_service) for Deferred engines"
    );
    #[cfg(feature = "instrumentation")]
    crate::instrumentation::ttl::record_non_native_warn(capability_label, engine_id);
}

/// Tables in `registry` whose schema declares `ttl:`.
#[must_use]
pub fn list_ttl_table_names(registry: &SchemaRegistry) -> Vec<String> {
    registry
        .list_schemas()
        .into_iter()
        .filter(|t| {
            registry
                .get_schema(t)
                .and_then(|meta| meta.schema.ttl.as_ref())
                .is_some()
        })
        .map(str::to_string)
        .collect()
}

/// Apply or warn for every schema in the registry that declares `ttl:`.
///
/// Primary host wire-up: call once after backends are registered. Uses
/// [`SchemaRegistry::list_schemas`] (inventory / `auto_discover`) — no hand-maintained table list.
///
/// # Errors
///
/// Returns the first error from [`ensure_ttl_for_table`] (for example native index creation).
/// Non-native backends warn and still return [`Ok`].
pub async fn ensure_ttl_for_all(valence: &Valence) -> Result<()> {
    let tables = list_ttl_table_names(SchemaRegistry::global());
    let ttl_schema_count = tables.len();
    tracing::debug!(
        target: "valence_ttl",
        ttl_schema_count,
        "ttl.ensure_all start"
    );
    #[cfg(feature = "instrumentation")]
    crate::instrumentation::ttl::record_ensure_all(ttl_schema_count);
    for table in tables {
        ensure_ttl_for_table(valence, &table).await?;
    }
    Ok(())
}

/// Apply native TTL policy for `table`, or warn once when the backend is Deferred/Unsupported.
///
/// # Errors
///
/// Propagates [`crate::DatabaseBackend::apply_ttl_policy`] failures on native backends
/// (mapped as [`crate::Error::Database`] by adapters). Router/backend resolution failures
/// surface as [`crate::Error::Internal`] / database errors from [`Valence::backend_for_table`].
pub async fn ensure_ttl_for_table(valence: &Valence, table: &str) -> Result<()> {
    let Some(policy) = policy_for_table(table) else {
        return Ok(());
    };
    let backend = valence.backend_for_table(table)?;
    ensure_ttl_for_table_on_backend(table, backend.as_ref(), &policy).await
}

pub(crate) async fn ensure_ttl_for_table_on_backend(
    table: &str,
    backend: &dyn DatabaseBackend,
    policy: &super::SchemaTtlPolicy,
) -> Result<()> {
    let capability = backend.ttl_capability();
    let engine_id = backend.engine_id();
    tracing::debug!(
        target: "valence_ttl",
        table,
        engine_id,
        capability = ?capability,
        "ttl.ensure"
    );
    #[cfg(feature = "instrumentation")]
    {
        let label = match capability {
            BackendTtlCapability::SupportedNative => "native",
            BackendTtlCapability::Deferred => "deferred",
            BackendTtlCapability::Unsupported => "unsupported",
        };
        crate::instrumentation::ttl::record_ensure(label, engine_id);
    }
    match capability {
        BackendTtlCapability::SupportedNative => {
            backend.apply_ttl_policy(table, policy).await?;
            tracing::debug!(
                target: "valence_ttl",
                table,
                engine_id,
                seconds = policy.seconds,
                "ttl.apply_native ok"
            );
            Ok(())
        }
        BackendTtlCapability::Deferred => {
            // Expire-at index for platform sweeper discovery (idempotent).
            backend.apply_ttl_policy(table, policy).await?;
            warn_non_native_once(table, engine_id, capability);
            Ok(())
        }
        BackendTtlCapability::Unsupported => {
            warn_non_native_once(table, engine_id, capability);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluator::DEFAULT_IN_MEMORY;
    use crate::schema::SchemaMetadata;
    use crate::schema_api::{Schema, SchemaField, SchemaMeta, SchemaPrivacy};
    use crate::ttl::{BackendTtlCapability, SchemaTtlPolicy};

    static WARN_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset_warn_state() {
        reset_ttl_warn_state_for_tests();
    }

    fn lock_warn_tests() -> std::sync::MutexGuard<'static, ()> {
        WARN_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn policy() -> SchemaTtlPolicy {
        SchemaTtlPolicy {
            seconds: 60,
            mode: "backend_capability".into(),
        }
    }

    fn leak_schema(name: &str, ttl: Option<SchemaTtlPolicy>) -> &'static Schema {
        Box::leak(Box::new(Schema {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            databases: vec!["default".to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "service".to_string(),
            },
            policies: None,
            fields: vec![SchemaField {
                name: "id".to_string(),
                field_type: "string".to_string(),
                primary: true,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: None,
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            }],
            edges: Vec::new(),
            connections: Vec::new(),
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            ttl,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: None,
            },
        }))
    }

    #[derive(Debug)]
    struct FakeBackend {
        capability: BackendTtlCapability,
        apply_calls: std::sync::Mutex<u32>,
        apply_err: bool,
    }

    #[async_trait::async_trait]
    impl DatabaseBackend for FakeBackend {
        fn engine_id(&self) -> &'static str {
            "fake_ttl"
        }
        fn capabilities(&self) -> crate::BackendCapabilities {
            crate::BackendCapabilities {
                supports_merge: false,
                supports_graph_edges: false,
                telemetry_label: "fake",
            }
        }
        async fn execute_compiled_query(
            &self,
            _: &crate::CompiledQuery,
        ) -> Result<Vec<serde_json::Value>> {
            Ok(vec![])
        }
        async fn get_record(&self, _: &str, _: &str) -> Result<Option<serde_json::Value>> {
            Ok(None)
        }
        async fn create_record(
            &self,
            _: &str,
            content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(content)
        }
        async fn update_record(
            &self,
            _: &str,
            _: &str,
            content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(content)
        }
        async fn upsert_record(
            &self,
            _: &str,
            _: &str,
            content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(content)
        }
        async fn delete_record(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        async fn relate_edge(
            &self,
            _: &crate::RecordId,
            _: &str,
            _: &crate::RecordId,
        ) -> Result<()> {
            Ok(())
        }
        async fn unrelate_edge(
            &self,
            _: &crate::RecordId,
            _: &str,
            _: &crate::RecordId,
        ) -> Result<()> {
            Ok(())
        }
        async fn get_edge_targets(
            &self,
            _: &crate::RecordId,
            _: &str,
        ) -> Result<Vec<crate::RecordId>> {
            Ok(vec![])
        }
        fn ttl_capability(&self) -> BackendTtlCapability {
            self.capability
        }
        async fn apply_ttl_policy(&self, _: &str, _: &SchemaTtlPolicy) -> Result<()> {
            *self.apply_calls.lock().unwrap() += 1;
            if self.apply_err {
                return Err(crate::Error::database("ttl index failed"));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn ensure_native_calls_apply() {
        let backend = FakeBackend {
            capability: BackendTtlCapability::SupportedNative,
            apply_calls: std::sync::Mutex::new(0),
            apply_err: false,
        };
        ensure_ttl_for_table_on_backend("t", &backend, &policy())
            .await
            .unwrap();
        assert_eq!(*backend.apply_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn ensure_deferred_applies_index_policy() {
        let _lock = lock_warn_tests();
        reset_warn_state();
        let backend = FakeBackend {
            capability: BackendTtlCapability::Deferred,
            apply_calls: std::sync::Mutex::new(0),
            apply_err: false,
        };
        ensure_ttl_for_table_on_backend("t_deferred_warn", &backend, &policy())
            .await
            .unwrap();
        assert_eq!(*backend.apply_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn ensure_deferred_warns_once_ok() {
        let _lock = lock_warn_tests();
        reset_warn_state();
        let backend = FakeBackend {
            capability: BackendTtlCapability::Deferred,
            apply_calls: std::sync::Mutex::new(0),
            apply_err: false,
        };
        ensure_ttl_for_table_on_backend("t_warn_once", &backend, &policy())
            .await
            .unwrap();
        ensure_ttl_for_table_on_backend("t_warn_once", &backend, &policy())
            .await
            .unwrap();
        assert_eq!(ttl_warn_emit_count_for_tests(), 1);
        assert_eq!(*backend.apply_calls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn ensure_unsupported_warns_once_ok() {
        let _lock = lock_warn_tests();
        reset_warn_state();
        let backend = FakeBackend {
            capability: BackendTtlCapability::Unsupported,
            apply_calls: std::sync::Mutex::new(0),
            apply_err: false,
        };
        ensure_ttl_for_table_on_backend("t_unsupported_warn", &backend, &policy())
            .await
            .unwrap();
        ensure_ttl_for_table_on_backend("t_unsupported_warn", &backend, &policy())
            .await
            .unwrap();
        assert_eq!(ttl_warn_emit_count_for_tests(), 1);
        assert_eq!(*backend.apply_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn ensure_native_apply_error_is_database() {
        let backend = FakeBackend {
            capability: BackendTtlCapability::SupportedNative,
            apply_calls: std::sync::Mutex::new(0),
            apply_err: true,
        };
        let err = ensure_ttl_for_table_on_backend("t_err", &backend, &policy())
            .await
            .unwrap_err();
        assert!(matches!(err, crate::Error::Database { .. }));
        assert_eq!(*backend.apply_calls.lock().unwrap(), 1);
    }

    #[test]
    fn list_ttl_tables_filters_registry() {
        let mut registry = SchemaRegistry::new();
        assert!(list_ttl_table_names(&registry).is_empty());

        registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(
            leak_schema("no_ttl", None),
        ))));
        assert!(list_ttl_table_names(&registry).is_empty());

        registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(
            leak_schema("with_ttl_a", Some(policy())),
        ))));
        registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(
            leak_schema("with_ttl_b", Some(policy())),
        ))));
        let names = list_ttl_table_names(&registry);
        assert_eq!(
            names,
            vec!["with_ttl_a".to_string(), "with_ttl_b".to_string()]
        );
    }
}
