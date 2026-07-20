//! Shared types, constants, and helpers for ownership persistence.

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::actor::Actor;
use crate::error::{Error, Result};
use crate::owner_ref::{OwnerKind, OwnerRef};
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;
use crate::RecordId;

/// Tables that must not receive nested ownership rows.
pub const SKIP_OWNERSHIP_TABLES: &[&str] =
    &["valence_data_ownership", "valence_ownership_transfer"];

pub fn skip_ownership_for_table(table: &str) -> bool {
    SKIP_OWNERSHIP_TABLES.contains(&table)
}

/// Stable namespace for UUIDv5 ownership row ids (not a secret).
const OWNERSHIP_ROW_UUID_NS: Uuid = Uuid::from_u128(0xe7b3c1d0_f2a4_5b8e_9c6d_a1b2e3f40516);

/// Deterministic primary key for `valence_data_ownership` from (`valence_model`, `record_id`).
pub fn ownership_row_id(valence_model: &str, record_id: &str) -> String {
    let name = format!("{valence_model}\n{record_id}");
    Uuid::new_v5(&OWNERSHIP_ROW_UUID_NS, name.as_bytes()).to_string()
}

pub fn system_valence(v: &Valence) -> Valence {
    v.with_actor(Actor::System {
        operation: "valence_data_ownership".to_string(),
    })
}

/// Surreal `SELECT VALUE` may return string literals with SQL-style wrapping quotes.
pub fn normalize_pending_deletion_query_value(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'\'' && bytes[s.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[s.len() - 1] == b'"')
        {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Whether ownership rows are co-located with the owning model's logical store (default on;
/// set `VALENCE_OWNERSHIP_COLOCATE=0` to disable).
pub fn ownership_colocate_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("VALENCE_OWNERSHIP_COLOCATE").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        )
    })
}

/// Whether generated `Model::get` uses a single compiled query for the main row plus ownership
/// gate status (default on; set `VALENCE_OWNERSHIP_UNIFIED_FETCH=0` to revert to legacy two-trip path).
pub fn ownership_unified_fetch_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("VALENCE_OWNERSHIP_UNIFIED_FETCH").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        )
    })
}

/// Privacy errors take precedence over pending-deletion. N6 soak: modest ownership slow-op reduction;
/// does not flatten HTTP 1 h growth slope alone.
pub fn ownership_get_join_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("VALENCE_OWNERSHIP_GET_JOIN").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        )
    })
}

/// Ownership gate input bundled with a unified `Model::get` fetch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnershipGateStatus {
    /// Legacy path: status was not fetched with the row (caller runs `pending_deletion_gate`).
    NotFetched,
    /// No ownership row exists for the record.
    Absent,
    /// Ownership row present with the given status string.
    Status(String),
}

impl OwnershipGateStatus {
    pub fn is_pending_deletion(&self) -> bool {
        matches!(self, Self::Status(s) if s == "pending_deletion")
    }

    #[must_use]
    pub(crate) fn from_optional_status(raw: Option<Value>) -> Self {
        match raw.and_then(|v| v.as_str().map(str::to_string)) {
            Some(s) => Self::Status(s),
            None => Self::Absent,
        }
    }
}

/// Row + ownership gate inputs from a unified `Model::get` fetch.
#[derive(Clone, Debug)]
pub struct RecordOwnershipBundle {
    pub row: Option<Value>,
    pub ownership_status: OwnershipGateStatus,
}

/// Per-schema owned row counts for one owner.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct OwnerSchemaRowCount {
    pub valence_model: String,
    pub active_rows: u64,
    pub pending_deletion_rows: u64,
}

/// Ownership footprint for one owner across registered app schemas.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct OwnerDataSummary {
    pub owned_rows: u64,
    pub tables_with_data: u64,
    pub pending_deletion_rows: u64,
    pub rows_by_schema: Vec<OwnerSchemaRowCount>,
}

pub const OWNER_SUMMARY_CONCURRENCY: usize = 8;

pub fn schema_skipped_for_owner_summary(table: &str, schema: &crate::Schema) -> bool {
    skip_ownership_for_table(table) || schema.ownership.as_ref().is_some_and(|o| o.system_owned)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "backend count values may be represented as floating-point JSON"
)]
pub fn parse_count_from_row(row: &Value) -> u64 {
    row.get("n")
        .or_else(|| row.get("count"))
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
        .unwrap_or(0)
}

fn push_unique_owner_id(values: &mut Vec<String>, id: impl Into<String>) {
    let id = id.into();
    if !values.iter().any(|v| v == &id) {
        values.push(id);
    }
}

/// Collect `owner_id` strings that may refer to the same user principal in ownership rows.
pub fn owner_id_query_values(owner_id: &str, owner_type: &str) -> Vec<String> {
    let mut values = vec![owner_id.to_string()];
    if owner_type != OwnerKind::User.as_str() {
        return values;
    }
    if let Some(bare) = owner_id.strip_prefix("user:") {
        if !bare.is_empty() {
            push_unique_owner_id(&mut values, bare);
        }
    } else if !owner_id.contains(':') {
        push_unique_owner_id(&mut values, format!("user:{owner_id}"));
    }
    values
}

/// Map stored `owner_type` string to [`OwnerKind`] for UI.
pub fn parse_owner_kind(s: &str) -> OwnerKind {
    match s {
        "user" => OwnerKind::User,
        "account" => OwnerKind::Account,
        "application" => OwnerKind::Application,
        "service" => OwnerKind::Service,
        _ => OwnerKind::System,
    }
}

/// Build an [`OwnerRef`] from a stored ownership JSON object.
pub fn owner_ref_from_ownership_json(v: &Value) -> Option<OwnerRef> {
    let owner_id = v.get("owner_id")?.as_str()?.to_string();
    let t = v.get("owner_type")?.as_str()?;
    Some(OwnerRef {
        owner_id,
        owner_kind: parse_owner_kind(t),
    })
}

/// Normalize a primary key string for ownership keys and deletion graphs.
pub fn normalize_record_id_for_ownership(entity_id: &str) -> String {
    let s = entity_id.trim();
    let Some((head, rest)) = s.split_once(':') else {
        return s.to_string();
    };
    if head.is_empty() || rest.is_empty() {
        return s.to_string();
    }
    if SchemaRegistry::global().get_schema(head).is_some() {
        return rest.to_string();
    }
    if !rest.contains(':') {
        return rest.to_string();
    }
    s.to_string()
}

/// Build transfer audit row JSON and append to `valence_ownership_transfer`.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn append_transfer_history_row(
    valence_model: &str,
    record_id: &str,
    oid: &str,
    from_owner_id: &str,
    from_owner_type: &str,
    new_owner: &OwnerRef,
    reason: Option<String>,
    v: &Valence,
) -> Result<()> {
    let transferred_by = match v.actor() {
        Actor::User { user_id } => user_id.clone(),
        Actor::ServiceUser { service_name } => format!("service:{service_name}"),
        Actor::System { operation } => format!("system:{operation}"),
        Actor::Anonymous => "anonymous".to_string(),
    };

    let tid = Uuid::new_v4().to_string();
    let rid = RecordId::new("valence_data_ownership", oid);
    let transfer = json!({
        "id": tid,
        "ownership_id": rid,
        "from_owner_id": from_owner_id,
        "from_owner_type": from_owner_type,
        "to_owner_id": new_owner.owner_id,
        "to_owner_type": new_owner.owner_kind.as_str(),
        "transferred_at": Utc::now(),
        "transferred_by": transferred_by,
        "reason": reason,
    });
    let sys = system_valence(v);
    let tback = sys.backend_for_table("valence_ownership_transfer")?;
    tback
        .create_record("valence_ownership_transfer", transfer)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

    crate::instrumentation::record_ownership_transfer(
        valence_model,
        record_id,
        from_owner_id,
        from_owner_type,
        &new_owner.owner_id,
        new_owner.owner_kind.as_str(),
        &transferred_by,
    );
    Ok(())
}
