//! Row ownership persistence (`valence_data_ownership` / `valence_ownership_transfer`).
//!
//! Uses the database backend directly for these platform tables so `valence` does not need a
//! self-hosted `valence-codegen` pass (which would create a Cargo build dependency cycle).

mod gates;
mod helpers;
mod read;
mod write;

pub use helpers::{
    normalize_record_id_for_ownership, owner_ref_from_ownership_json, ownership_colocate_enabled,
    ownership_get_join_enabled, ownership_unified_fetch_enabled, parse_owner_kind,
    skip_ownership_for_table, OwnerDataSummary, OwnerSchemaRowCount, OwnershipGateStatus,
    RecordOwnershipBundle, SKIP_OWNERSHIP_TABLES,
};

use std::sync::{Arc, Mutex, PoisonError, Weak};

use crate::backend::DatabaseBackend;
use crate::error::{Error, Result};
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

use helpers::system_valence;

/// Ownership persistence and transfer helpers.
pub struct OwnershipService;

/// Fields every ownership filter pins: the pending-deletion check and the owner rollups
/// both match on model and status before anything else.
const LOOKUP_INDEX_FIELDS: &[&str] = &["valence_model", "status"];

/// Ensure the ownership lookup index exists on every backend that stores ownership rows
/// (idempotent).
///
/// Ownership reads create the index on first use, so hosts don't need to call this; it is
/// for a host that wants the index in place before the first request.
/// # Errors
///
/// Returns an error when a backend fails to create the index.
pub async fn ensure_lookup_indexes(v: &Valence) -> Result<()> {
    let sys = system_valence(v);
    let mut backends: Vec<Arc<dyn DatabaseBackend>> = Vec::new();

    let mut push_unique = |backend: Arc<dyn DatabaseBackend>| {
        if !backends.iter().any(|b| Arc::ptr_eq(b, &backend)) {
            backends.push(backend);
        }
    };

    if let Ok(backend) = sys.backend_for_table("valence_data_ownership") {
        push_unique(backend);
    }

    if ownership_colocate_enabled() {
        for table in SchemaRegistry::global().list_schemas() {
            if skip_ownership_for_table(table) {
                continue;
            }
            if let Ok(backend) = OwnershipService::ownership_backend(table, &sys) {
                push_unique(backend);
            }
        }
    }

    for backend in backends {
        backend
            .define_index("valence_data_ownership", LOOKUP_INDEX_FIELDS)
            .await
            .map_err(|e| Error::database(e.to_string()))?;
    }
    Ok(())
}

/// Define the ownership lookup index on `backend` the first time this process uses it for
/// ownership.
///
/// A failure is logged and not retried: ownership queries return the same rows without the
/// index, they only scan the whole table.
pub(crate) async fn ensure_lookup_index_once(backend: &Arc<dyn DatabaseBackend>) {
    static SEEN: Mutex<Vec<Weak<dyn DatabaseBackend>>> = Mutex::new(Vec::new());
    {
        let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
        // Drop dead backends first so a new one at a reused address still gets its index.
        seen.retain(|w| w.strong_count() > 0);
        if seen
            .iter()
            .any(|w| std::ptr::addr_eq(w.as_ptr(), Arc::as_ptr(backend)))
        {
            return;
        }
        seen.push(Arc::downgrade(backend));
    }
    if let Err(e) = backend
        .define_index("valence_data_ownership", LOOKUP_INDEX_FIELDS)
        .await
    {
        tracing::warn!(
            engine = backend.engine_id(),
            error = %e,
            "valence ownership lookup index not created; ownership queries scan the whole table"
        );
    }
}

#[cfg(test)]
mod lookup_index_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::ensure_lookup_index_once;
    use crate::backend::{BackendCapabilities, DatabaseBackend};
    use crate::compiled_query::CompiledQuery;
    use crate::error::{Error, Result};
    use crate::record_id::RecordId;

    /// Counts `define_index` calls; optionally fails them.
    #[derive(Debug, Default)]
    struct CountingBackend {
        defines: AtomicUsize,
        fail: bool,
    }

    #[async_trait]
    impl DatabaseBackend for CountingBackend {
        fn engine_id(&self) -> &'static str {
            "counting"
        }
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities::mem()
        }
        async fn execute_compiled_query(
            &self,
            _: &CompiledQuery,
        ) -> Result<Vec<serde_json::Value>> {
            Ok(vec![])
        }
        async fn get_record(&self, _: &str, _: &str) -> Result<Option<serde_json::Value>> {
            Ok(None)
        }
        async fn create_record(&self, _: &str, c: serde_json::Value) -> Result<serde_json::Value> {
            Ok(c)
        }
        async fn update_record(
            &self,
            _: &str,
            _: &str,
            c: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(c)
        }
        async fn upsert_record(
            &self,
            _: &str,
            _: &str,
            c: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(c)
        }
        async fn delete_record(&self, _: &str, _: &str) -> Result<()> {
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
        async fn define_index(&self, table: &str, fields: &[&str]) -> Result<()> {
            assert_eq!(table, "valence_data_ownership");
            assert_eq!(fields, ["valence_model", "status"]);
            self.defines.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(Error::Internal("no index for you".into()));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn defines_once_per_backend_happy() {
        let a = Arc::new(CountingBackend::default());
        let b = Arc::new(CountingBackend::default());
        let (da, db): (Arc<dyn DatabaseBackend>, Arc<dyn DatabaseBackend>) = (a.clone(), b.clone());
        for _ in 0..3 {
            ensure_lookup_index_once(&da).await;
            ensure_lookup_index_once(&db).await;
        }
        assert_eq!(a.defines.load(Ordering::SeqCst), 1);
        assert_eq!(b.defines.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_define_is_not_retried_sad() {
        let a = Arc::new(CountingBackend {
            fail: true,
            ..CountingBackend::default()
        });
        let da: Arc<dyn DatabaseBackend> = a.clone();
        ensure_lookup_index_once(&da).await;
        ensure_lookup_index_once(&da).await;
        assert_eq!(a.defines.load(Ordering::SeqCst), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::helpers::{
        normalize_record_id_for_ownership, owner_id_query_values, owner_ref_from_ownership_json,
        parse_owner_kind, skip_ownership_for_table,
    };
    use crate::owner_ref::{OwnerKind, OwnerRef};
    use serde_json::json;

    #[test]
    fn owner_id_query_values_matches_session_and_signup_user_ids() {
        assert_eq!(
            owner_id_query_values("user:abc-123", "user"),
            vec!["user:abc-123".to_string(), "abc-123".to_string()]
        );
        assert_eq!(
            owner_id_query_values("abc-123", "user"),
            vec!["abc-123".to_string(), "user:abc-123".to_string()]
        );
        assert_eq!(
            owner_id_query_values("service-bot", "service"),
            vec!["service-bot".to_string()]
        );
    }

    #[test]
    fn parse_owner_kind_known_and_default() {
        assert_eq!(parse_owner_kind("user"), OwnerKind::User);
        assert_eq!(parse_owner_kind("account"), OwnerKind::Account);
        assert_eq!(parse_owner_kind("application"), OwnerKind::Application);
        assert_eq!(parse_owner_kind("service"), OwnerKind::Service);
        assert_eq!(parse_owner_kind("anything_else"), OwnerKind::System);
    }

    #[test]
    fn skip_ownership_for_platform_tables_only() {
        assert!(skip_ownership_for_table("valence_data_ownership"));
        assert!(skip_ownership_for_table("valence_ownership_transfer"));
        assert!(!skip_ownership_for_table("counter"));
    }

    #[test]
    fn owner_ref_from_ownership_json_extracts_fields() {
        let v = json!({
            "owner_id": "u1",
            "owner_type": "user",
        });
        let r = owner_ref_from_ownership_json(&v).expect("parsed");
        assert_eq!(r.owner_id, "u1");
        assert_eq!(r.owner_kind, OwnerKind::User);
        assert!(owner_ref_from_ownership_json(&json!({})).is_none());
    }

    #[test]
    fn normalize_record_id_strips_registered_table_prefix_only() {
        assert_eq!(normalize_record_id_for_ownership("bare-id"), "bare-id");
        // Unregistered `head:` must not be treated as Surreal table:id — app PKs
        // like `slug:trim` and `appsetmem-slug:trim--uuid` contain colons.
        assert_eq!(
            normalize_record_id_for_ownership("counter:singleton"),
            "counter:singleton"
        );
        assert_eq!(normalize_record_id_for_ownership("slug:trim"), "slug:trim");
        assert_eq!(
            normalize_record_id_for_ownership("appsetmem-slug:trim--app-uuid"),
            "appsetmem-slug:trim--app-uuid"
        );
    }

    #[test]
    fn normalize_record_id_keeps_composite_id_without_known_table() {
        assert_eq!(
            normalize_record_id_for_ownership("default-deployment:wiztop:0"),
            "default-deployment:wiztop:0"
        );
    }

    #[test]
    fn owner_ref_round_trip_from_stored_json() {
        let v = json!({"owner_id": "svc", "owner_type": "service"});
        let r = owner_ref_from_ownership_json(&v).unwrap();
        assert_eq!(
            r,
            OwnerRef {
                owner_id: "svc".into(),
                owner_kind: OwnerKind::Service,
            }
        );
    }

    #[test]
    fn ownership_get_join_enabled_defaults_on() {
        assert!(super::helpers::ownership_get_join_enabled());
    }

    #[test]
    fn ownership_colocate_enabled_defaults_on() {
        assert!(super::helpers::ownership_colocate_enabled());
    }

    #[test]
    fn pending_deletion_status_blocks_reads() {
        use super::helpers::OwnershipGateStatus;
        use super::OwnershipService;
        use crate::error::Error;

        let err = OwnershipService::apply_pending_deletion_gate(
            "widget",
            "w1",
            OwnershipGateStatus::Status("pending_deletion".into()),
        )
        .expect_err("pending deletion must block");
        assert!(matches!(err, Error::PendingDeletion(_)));

        OwnershipService::apply_pending_deletion_gate("widget", "w1", OwnershipGateStatus::Absent)
            .expect("absent ownership allows");
    }
}
