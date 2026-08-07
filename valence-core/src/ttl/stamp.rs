//! Create-time expire-at stamping for Deferred and Mongo-native TTL.

use chrono::{Duration, Utc};
use serde_json::{Map, Value};

use crate::backend::DatabaseBackend;
use crate::error::Result;
use crate::schema::SchemaRegistry;

use super::policy::{BackendTtlCapability, SchemaTtlPolicy};

/// Reserved document field holding RFC3339 UTC expire time (create-only).
pub const EXPIRE_AT_FIELD: &str = "__valence_expire_at";

/// Look up [`SchemaTtlPolicy`] for `table` from the global registry.
#[must_use]
pub fn policy_for_table(table: &str) -> Option<SchemaTtlPolicy> {
    SchemaRegistry::global()
        .get_schema(table)
        .and_then(|meta| meta.schema.ttl.clone())
}

/// Whether this capability should persist [`EXPIRE_AT_FIELD`] on create.
#[must_use]
pub fn should_stamp_expire_at(capability: BackendTtlCapability) -> bool {
    matches!(
        capability,
        BackendTtlCapability::Deferred | BackendTtlCapability::SupportedNative
    )
}

/// Insert [`EXPIRE_AT_FIELD`] = now + `policy.seconds` unless the field already exists.
///
/// Create-only: never overwrites an existing expire timestamp.
pub fn stamp_expire_at_if_absent(content: &mut Value, policy: &SchemaTtlPolicy) {
    let Some(obj) = content.as_object_mut() else {
        let mut map = Map::new();
        insert_expire(&mut map, policy);
        *content = Value::Object(map);
        return;
    };
    if obj.contains_key(EXPIRE_AT_FIELD) {
        return;
    }
    insert_expire(obj, policy);
}

fn insert_expire(obj: &mut Map<String, Value>, policy: &SchemaTtlPolicy) {
    let secs = i64::try_from(policy.seconds).unwrap_or(i64::MAX);
    let at = Utc::now() + Duration::seconds(secs);
    obj.insert(EXPIRE_AT_FIELD.to_string(), Value::String(at.to_rfc3339()));
}

/// Prepare `content` for create / creating-upsert when the table has a TTL policy.
///
/// Stamps [`EXPIRE_AT_FIELD`] when the backend capability is [`BackendTtlCapability::Deferred`]
/// or [`BackendTtlCapability::SupportedNative`]. No-op when the schema has no TTL or the
/// backend is [`BackendTtlCapability::Unsupported`]. Does not refresh an existing expire field.
///
/// # Errors
///
/// Currently infallible; returns [`Result`] for API stability with call sites that use `?`.
pub fn prepare_create_content(
    table: &str,
    backend: &dyn DatabaseBackend,
    content: &mut Value,
) -> Result<()> {
    prepare_create_content_with_capability(table, backend.ttl_capability(), content)
}

/// Same as [`prepare_create_content`] when the caller already knows [`BackendTtlCapability`].
///
/// # Errors
///
/// Currently infallible; returns [`Result`] for API stability with call sites that use `?`.
pub fn prepare_create_content_with_capability(
    table: &str,
    capability: BackendTtlCapability,
    content: &mut Value,
) -> Result<()> {
    let Some(policy) = policy_for_table(table) else {
        return Ok(());
    };
    if !should_stamp_expire_at(capability) {
        tracing::trace!(
            target: "valence_ttl",
            table,
            capability = ?capability,
            "ttl.stamp skipped"
        );
        return Ok(());
    }
    stamp_expire_at_if_absent(content, &policy);
    tracing::trace!(
        target: "valence_ttl",
        table,
        capability = ?capability,
        "ttl.stamp applied"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ttl::BackendTtlCapability;

    fn policy(seconds: u64) -> SchemaTtlPolicy {
        SchemaTtlPolicy {
            seconds,
            mode: "backend_capability".into(),
        }
    }

    #[test]
    fn stamps_expire_at_when_absent() {
        let mut v = serde_json::json!({"id": "a"});
        stamp_expire_at_if_absent(&mut v, &policy(1800));
        let s = v[EXPIRE_AT_FIELD].as_str().expect("expire field");
        let parsed = chrono::DateTime::parse_from_rfc3339(s).expect("rfc3339");
        let delta = parsed.with_timezone(&Utc) - Utc::now();
        assert!(delta.num_seconds() > 1700 && delta.num_seconds() <= 1800);
    }

    #[test]
    fn stamp_does_not_overwrite_existing_expire_at() {
        let mut v = serde_json::json!({
            "id": "a",
            EXPIRE_AT_FIELD: "2099-01-01T00:00:00Z"
        });
        stamp_expire_at_if_absent(&mut v, &policy(1));
        assert_eq!(v[EXPIRE_AT_FIELD], "2099-01-01T00:00:00Z");
    }

    #[test]
    fn should_stamp_for_deferred_and_native() {
        assert!(should_stamp_expire_at(BackendTtlCapability::Deferred));
        assert!(should_stamp_expire_at(
            BackendTtlCapability::SupportedNative
        ));
        assert!(!should_stamp_expire_at(BackendTtlCapability::Unsupported));
    }

    #[test]
    fn prepare_noop_without_schema_ttl() {
        let mut v = serde_json::json!({"id": "a"});
        prepare_create_content_with_capability(
            "no_such_ttl_table_zzz",
            BackendTtlCapability::Deferred,
            &mut v,
        )
        .unwrap();
        assert!(v.get(EXPIRE_AT_FIELD).is_none());
    }
}
