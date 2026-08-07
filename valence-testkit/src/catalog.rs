//! Shared correctness catalog for matrix E2E and bench smoke.

use crate::bootstrap::BootstrapSession;
use crate::matrix::{
    all_storage_adapters, extended_store_available, extended_store_skip_reason, topology_available,
    topology_skip_reason, MatrixSpec, StorageAdapter, TelemetryAdapter,
};
use crate::runner::{RunMode, ScenarioRunner};
use crate::scenario::ScenarioSpec;

/// Happy vs sad path label for catalog entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Expected success.
    Happy,
    /// Expected rejection or error path.
    Sad,
}

/// One row in the shared correctness catalog.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    /// Stable id (matches test name prefix).
    pub id: &'static str,
    /// Happy or sad path.
    pub path: PathKind,
    /// Scenario factory.
    pub spec: fn(StorageAdapter) -> ScenarioSpec,
    /// Optional telemetry override (defaults to Off).
    pub telemetry: Option<TelemetryAdapter>,
    /// When true, inventory bootstrap is used (Surreal only).
    pub inventory_bootstrap: bool,
    /// Extra logical names for multi-logical scenarios.
    pub logical_names: Option<&'static [&'static str]>,
    /// When true, skip unless storage supports the scenario.
    pub surreal_only: bool,
    /// When true, only run on adapters that support generated model CRUD.
    pub generated_model_only: bool,
}

const fn entry(
    id: &'static str,
    path: PathKind,
    spec: fn(StorageAdapter) -> ScenarioSpec,
) -> CatalogEntry {
    CatalogEntry {
        id,
        path,
        spec,
        telemetry: None,
        inventory_bootstrap: false,
        logical_names: None,
        surreal_only: false,
        generated_model_only: false,
    }
}

const fn entry_telemetry(
    id: &'static str,
    path: PathKind,
    spec: fn(StorageAdapter) -> ScenarioSpec,
    telemetry: TelemetryAdapter,
) -> CatalogEntry {
    CatalogEntry {
        id,
        path,
        spec,
        telemetry: Some(telemetry),
        inventory_bootstrap: false,
        logical_names: None,
        surreal_only: false,
        generated_model_only: false,
    }
}

const fn entry_inventory(
    id: &'static str,
    spec: fn(StorageAdapter) -> ScenarioSpec,
) -> CatalogEntry {
    CatalogEntry {
        id,
        path: PathKind::Happy,
        spec,
        telemetry: None,
        inventory_bootstrap: true,
        logical_names: None,
        surreal_only: true,
        generated_model_only: false,
    }
}

const fn entry_generated_model(
    id: &'static str,
    path: PathKind,
    spec: fn(StorageAdapter) -> ScenarioSpec,
) -> CatalogEntry {
    CatalogEntry {
        id,
        path,
        spec,
        telemetry: None,
        inventory_bootstrap: false,
        logical_names: None,
        surreal_only: false,
        generated_model_only: true,
    }
}

const fn entry_sad(id: &'static str, spec: fn(StorageAdapter) -> ScenarioSpec) -> CatalogEntry {
    CatalogEntry {
        id,
        path: PathKind::Sad,
        spec,
        telemetry: None,
        inventory_bootstrap: false,
        logical_names: None,
        surreal_only: false,
        generated_model_only: false,
    }
}

const fn entry_multi_logical(
    id: &'static str,
    spec: fn(StorageAdapter) -> ScenarioSpec,
    logical_names: &'static [&'static str],
) -> CatalogEntry {
    CatalogEntry {
        id,
        path: PathKind::Happy,
        spec,
        telemetry: None,
        inventory_bootstrap: false,
        logical_names: Some(logical_names),
        surreal_only: false,
        generated_model_only: false,
    }
}

fn router_multi_logical_spec(storage: StorageAdapter) -> ScenarioSpec {
    match storage {
        StorageAdapter::SurrealMem | StorageAdapter::SurrealRocksdb => {
            ScenarioSpec::router_multi_logical()
        }
        StorageAdapter::AcmeStub => ScenarioSpec::router_multi_logical_acme(),
        StorageAdapter::Mem => ScenarioSpec::router_multi_logical_mem(),
        #[cfg(feature = "sqlite")]
        StorageAdapter::Sqlite => ScenarioSpec::router_multi_logical_engine(
            valence_backend_sqlite::ENGINE_ID,
            &["default", "billing"],
        ),
        #[cfg(not(feature = "sqlite"))]
        StorageAdapter::Sqlite => ScenarioSpec::router_multi_logical_mem(),
        #[cfg(feature = "postgres")]
        StorageAdapter::Postgres => ScenarioSpec::router_multi_logical_engine(
            valence_backend_postgres::ENGINE_ID,
            &["default", "billing"],
        ),
        #[cfg(not(feature = "postgres"))]
        StorageAdapter::Postgres => ScenarioSpec::router_multi_logical_mem(),
        #[cfg(feature = "mongodb")]
        StorageAdapter::MongoDb => ScenarioSpec::router_multi_logical_engine(
            valence_backend_mongodb::ENGINE_ID,
            &["default", "billing"],
        ),
        #[cfg(not(feature = "mongodb"))]
        StorageAdapter::MongoDb => ScenarioSpec::router_multi_logical_mem(),
        #[cfg(feature = "indradb")]
        StorageAdapter::IndraDb => ScenarioSpec::router_multi_logical_engine(
            valence_backend_indradb::ENGINE_ID,
            &["default", "billing"],
        ),
        #[cfg(not(feature = "indradb"))]
        StorageAdapter::IndraDb => ScenarioSpec::router_multi_logical_mem(),
        #[cfg(feature = "redis")]
        StorageAdapter::Redis => ScenarioSpec::router_multi_logical_engine(
            valence_backend_redis::ENGINE_ID,
            &["default", "billing"],
        ),
        #[cfg(not(feature = "redis"))]
        StorageAdapter::Redis => ScenarioSpec::router_multi_logical_mem(),
        #[cfg(feature = "hybrid")]
        StorageAdapter::HybridIndraPg => ScenarioSpec::router_multi_logical_engine(
            valence_backend_hybrid::ENGINE_ID,
            &["default", "billing"],
        ),
        #[cfg(not(feature = "hybrid"))]
        StorageAdapter::HybridIndraPg => ScenarioSpec::router_multi_logical_mem(),
    }
}

/// Default embedded catalog exercised by `valence-e2e`.
pub fn embedded_catalog() -> &'static [CatalogEntry] {
    static CATALOG: &[CatalogEntry] = &[
        entry("builder-smoke", PathKind::Happy, |_| {
            ScenarioSpec::builder_smoke()
        }),
        entry_multi_logical(
            "router-multi-logical",
            router_multi_logical_spec,
            &["default", "billing"],
        ),
        entry_inventory("inventory-bootstrap", |_| {
            ScenarioSpec::inventory_bootstrap()
        }),
        entry_telemetry(
            "telemetry-crud-counters",
            PathKind::Happy,
            |_| ScenarioSpec::telemetry_crud_counters(),
            TelemetryAdapter::Recording,
        ),
        entry("factory-background-build", PathKind::Happy, |_| {
            ScenarioSpec::factory_background_build()
        }),
        entry("endpoint-env-resolve", PathKind::Happy, |_| {
            ScenarioSpec::endpoint_env_resolve()
        }),
        entry("compiled-query-empty-table", PathKind::Happy, |_| {
            ScenarioSpec::compiled_query_empty_table()
        }),
        entry_sad("router-key-not-found", ScenarioSpec::router_key_not_found),
        entry_sad("get-record-missing", |_| ScenarioSpec::get_record_missing()),
        entry_sad("privacy-read-deny-anonymous", |_| {
            ScenarioSpec::privacy_read_deny_anonymous()
        }),
        entry_generated_model("model-crud-smoke", PathKind::Happy, |_| {
            ScenarioSpec::model_crud_smoke()
        }),
        entry("ownership-gate-smoke", PathKind::Happy, |_| {
            ScenarioSpec::ownership_gate_smoke()
        }),
        entry("validation-reject-smoke", PathKind::Happy, |_| {
            ScenarioSpec::validation_reject_smoke()
        }),
        entry("graph-edge-smoke", PathKind::Happy, |_| {
            ScenarioSpec::graph_edge_smoke()
        }),
        entry_telemetry(
            "telemetry-console-smoke",
            PathKind::Happy,
            |_| ScenarioSpec::telemetry_console_smoke(),
            TelemetryAdapter::Console,
        ),
        entry("builder-empty-rejects", PathKind::Sad, |_| {
            ScenarioSpec::builder_empty_rejects()
        }),
        entry("endpoint-env-unresolved", PathKind::Sad, |_| {
            ScenarioSpec::endpoint_env_unresolved()
        }),
        entry("privacy-write-deny", PathKind::Sad, |_| {
            ScenarioSpec::privacy_write_deny()
        }),
        entry_sad("privacy-empty-default-deny", |_| {
            ScenarioSpec::privacy_empty_default_deny()
        }),
        entry_sad("privacy-field-system-only-hidden", |_| {
            ScenarioSpec::privacy_field_system_only_hidden()
        }),
        entry_sad("query-limit-clamped", |_| {
            ScenarioSpec::query_limit_clamped()
        }),
        entry_sad("privacy-bypass-requires-force", |_| {
            ScenarioSpec::privacy_bypass_requires_force()
        }),
        entry("validation-accept-smoke", PathKind::Happy, |_| {
            ScenarioSpec::validation_accept_smoke()
        }),
        entry_generated_model("model-update-upsert", PathKind::Happy, |_| {
            ScenarioSpec::model_update_upsert()
        }),
        entry_generated_model("query-filter-eq", PathKind::Happy, |_| {
            ScenarioSpec::query_filter_eq()
        }),
        entry_generated_model("query-filter-miss", PathKind::Sad, |_| {
            ScenarioSpec::query_filter_miss()
        }),
        entry_generated_model("query-order-by", PathKind::Happy, |_| {
            ScenarioSpec::query_order_by()
        }),
        entry_generated_model("query-pagination", PathKind::Happy, |_| {
            ScenarioSpec::query_pagination()
        }),
        entry_generated_model("query-offset-empty", PathKind::Sad, |_| {
            ScenarioSpec::query_offset_empty()
        }),
        entry_generated_model("read-cache-smoke", PathKind::Happy, |_| {
            ScenarioSpec::read_cache_smoke()
        }),
        entry("query-union-join-smoke", PathKind::Happy, |_| {
            ScenarioSpec::query_union_join_smoke()
        }),
        entry("m2m-relate-smoke", PathKind::Happy, |_| {
            ScenarioSpec::m2m_relate_smoke()
        }),
        entry("ttl-native-expire", PathKind::Happy, |storage| {
            ScenarioSpec::ttl_native_expire(ttl_record_id(storage, "native"))
        }),
        entry("ttl-deferred-stamp", PathKind::Happy, |storage| {
            ScenarioSpec::ttl_deferred_stamp(ttl_record_id(storage, "deferred"))
        }),
        entry("ttl-create-only-no-refresh", PathKind::Happy, |storage| {
            ScenarioSpec::ttl_create_only_no_refresh(ttl_record_id(storage, "create_only"))
        }),
        entry_sad("ttl-non-native-warn", |_| {
            ScenarioSpec::ttl_non_native_warn()
        }),
        entry("ttl-deferred-sweep-delete", PathKind::Happy, |storage| {
            ScenarioSpec::ttl_deferred_sweep_delete(ttl_record_id(storage, "sweep_delete"))
        }),
        entry("iter-scan-complete", PathKind::Happy, |_| {
            ScenarioSpec::iter_scan_complete()
        }),
        entry("on-delete-cascade-same-backend", PathKind::Happy, |_| {
            ScenarioSpec::on_delete_cascade_same_backend()
        }),
        entry("on-delete-set-null", PathKind::Happy, |_| {
            ScenarioSpec::on_delete_set_null()
        }),
        entry("on-delete-remove-edge", PathKind::Happy, |_| {
            ScenarioSpec::on_delete_remove_edge()
        }),
        entry_sad("on-delete-restrict-blocks", |_| {
            ScenarioSpec::on_delete_restrict_blocks()
        }),
        entry("on-delete-cascade-cross-engine", PathKind::Happy, |_| {
            ScenarioSpec::on_delete_cascade_cross_engine()
        }),
        entry("on-delete-set-null-cross-engine", PathKind::Happy, |_| {
            ScenarioSpec::on_delete_set_null_cross_engine()
        }),
    ];
    CATALOG
}

fn ttl_record_id(storage: StorageAdapter, kind: &str) -> String {
    format!("ttl_{}_{kind}", storage.slug())
}

/// Whether a TTL catalog entry applies to this storage adapter.
fn ttl_catalog_applies(entry_id: &str, storage: StorageAdapter) -> bool {
    if !entry_id.starts_with("ttl-") {
        return true;
    }
    if matches!(storage, StorageAdapter::AcmeStub) {
        return false;
    }
    match entry_id {
        "ttl-native-expire" => matches!(storage, StorageAdapter::Redis | StorageAdapter::MongoDb),
        "ttl-deferred-stamp" => !matches!(storage, StorageAdapter::Redis | StorageAdapter::MongoDb),
        "ttl-deferred-sweep-delete" => matches!(
            storage,
            StorageAdapter::Mem
                | StorageAdapter::Sqlite
                | StorageAdapter::Postgres
                | StorageAdapter::HybridIndraPg
                | StorageAdapter::SurrealMem
                | StorageAdapter::SurrealRocksdb
        ),
        "ttl-create-only-no-refresh" => !matches!(storage, StorageAdapter::IndraDb),
        "ttl-non-native-warn" => {
            !matches!(storage, StorageAdapter::Redis | StorageAdapter::MongoDb)
        }
        _ => true,
    }
}

/// Whether an iter catalog entry applies to this storage adapter.
fn iter_catalog_applies(entry_id: &str, storage: StorageAdapter) -> bool {
    if entry_id != "iter-scan-complete" {
        return true;
    }
    // Platform paging for redis/mongo/indra is unsupported until keyset pushdown exists.
    !matches!(
        storage,
        StorageAdapter::Redis
            | StorageAdapter::MongoDb
            | StorageAdapter::IndraDb
            | StorageAdapter::AcmeStub
    )
}

/// Whether an OnDelete catalog entry applies to this storage adapter.
fn on_delete_catalog_applies(entry_id: &str, storage: StorageAdapter) -> bool {
    if !entry_id.starts_with("on-delete-") {
        return true;
    }
    if matches!(storage, StorageAdapter::AcmeStub) {
        return false;
    }
    if entry_id.contains("cross-engine") {
        return crate::on_delete::on_delete_cross_engine_secondary(storage).is_some();
    }
    true
}

/// Storage adapters participating in default PR CI matrix.
#[must_use]
pub fn e2e_storage_backends() -> Vec<StorageAdapter> {
    all_storage_adapters()
        .into_iter()
        .filter(|s| extended_store_available(*s))
        .collect()
}

fn matrix_for_entry(entry: &CatalogEntry, storage: StorageAdapter) -> MatrixSpec {
    let telemetry = entry.telemetry.unwrap_or(TelemetryAdapter::Off);
    MatrixSpec {
        storage,
        telemetry,
        topology: crate::matrix::Topology::Embedded,
    }
}

/// Run one catalog entry for the given storage adapter.
///
/// # Panics
///
/// Panics on bootstrap failure or scenario assertion mismatch.
pub async fn run_catalog_entry(entry: &CatalogEntry, storage: StorageAdapter) {
    if entry.surreal_only
        && !matches!(
            storage,
            StorageAdapter::SurrealMem | StorageAdapter::SurrealRocksdb
        )
    {
        return;
    }

    if entry.inventory_bootstrap
        && !matches!(
            storage,
            StorageAdapter::SurrealMem | StorageAdapter::SurrealRocksdb
        )
    {
        return;
    }

    if entry.generated_model_only && !storage.supports_model_runtime() {
        return;
    }

    if !ttl_catalog_applies(entry.id, storage) {
        return;
    }
    if !iter_catalog_applies(entry.id, storage) {
        eprintln!(
            "catalog entry {}/{}: iter unsupported on this adapter until paging fix — skipping",
            entry.id,
            storage.slug()
        );
        return;
    }
    if !on_delete_catalog_applies(entry.id, storage) {
        return;
    }

    let matrix = matrix_for_entry(entry, storage);
    if let Some(reason) = extended_store_skip_reason(matrix.storage) {
        eprintln!(
            "catalog entry {}/{}: {reason} — skipping",
            entry.id,
            matrix.storage.slug()
        );
        return;
    }
    if let Some(reason) = topology_skip_reason(matrix.topology) {
        eprintln!(
            "catalog entry {}/{}: {reason} — skipping",
            entry.id,
            matrix.topology.slug()
        );
        return;
    }
    if !extended_store_available(matrix.storage) || !topology_available(matrix.topology) {
        return;
    }

    let _harness_guard = crate::harness_lock::lock_harness().await;
    let mut session = BootstrapSession::new(matrix);
    if let Some(names) = entry.logical_names {
        let names: &[&str] = match storage {
            StorageAdapter::AcmeStub if entry.id == "router-multi-logical" => &["primary", "vault"],
            _ => names,
        };
        session = session.with_logical_names(names);
    }
    #[cfg(feature = "surreal-inventory")]
    if entry.inventory_bootstrap {
        session = session.with_inventory_bootstrap();
    }
    #[cfg(not(feature = "surreal-inventory"))]
    if entry.inventory_bootstrap {
        eprintln!(
            "catalog entry {}: surreal-inventory feature disabled — skipping",
            entry.id
        );
        return;
    }

    session.spawn().await.expect("bootstrap spawn");
    let spec = (entry.spec)(storage);
    let mut runner = ScenarioRunner::new(&mut session);
    let result = runner
        .run(&spec, RunMode::Correctness)
        .await
        .expect("scenario run");

    assert!(
        result.error.is_none(),
        "scenario {} ({:?}) failed: {:?}",
        entry.id,
        entry.path,
        result.error
    );
}

/// Filter catalog entries applicable to a storage adapter.
pub fn catalog_for_storage(storage: StorageAdapter) -> Vec<&'static CatalogEntry> {
    embedded_catalog()
        .iter()
        .filter(|entry| {
            if entry.surreal_only
                && !matches!(
                    storage,
                    StorageAdapter::SurrealMem | StorageAdapter::SurrealRocksdb
                )
            {
                return false;
            }
            if entry.inventory_bootstrap
                && !matches!(
                    storage,
                    StorageAdapter::SurrealMem | StorageAdapter::SurrealRocksdb
                )
            {
                return false;
            }
            if entry.generated_model_only && !storage.supports_model_runtime() {
                return false;
            }
            if !ttl_catalog_applies(entry.id, storage) {
                return false;
            }
            if !on_delete_catalog_applies(entry.id, storage) {
                return false;
            }
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_twelve_plus_scenarios() {
        assert!(embedded_catalog().len() >= 12);
    }

    #[test]
    fn catalog_includes_sad_path_entries() {
        assert!(embedded_catalog().iter().any(|e| e.path == PathKind::Sad));
    }
}
