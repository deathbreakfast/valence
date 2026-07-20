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

use std::sync::Arc;

use crate::backend::DatabaseBackend;
use crate::error::{Error, Result};
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

use helpers::system_valence;

/// Ownership persistence and transfer helpers.
pub struct OwnershipService;

/// Ensure ownership lookup indexes exist when the active backend supports them (idempotent).
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn ensure_lookup_indexes(v: &Valence) -> Result<()> {
    let sys = system_valence(v);
    let q = concat!(
        "DEFINE INDEX IF NOT EXISTS idx_valence_data_ownership_model_status ",
        "ON TABLE valence_data_ownership COLUMNS valence_model, status"
    );
    let compiled = crate::compiled_query::CompiledQuery::new(q.to_string(), vec![]);

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
            .execute_compiled_query(&compiled)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
    }
    Ok(())
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
    fn normalize_record_id_strips_table_prefix() {
        assert_eq!(
            normalize_record_id_for_ownership("counter:singleton"),
            "singleton"
        );
        assert_eq!(normalize_record_id_for_ownership("bare-id"), "bare-id");
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
}
