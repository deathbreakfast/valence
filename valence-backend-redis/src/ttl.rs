//! Redis native TTL helpers (create-only `EXPIRE`).

use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use valence_core::ttl::{policy_for_table, BackendTtlCapability};
use valence_core::{Error, Result};

use crate::keys::Keyspace;

/// Redis reports [`BackendTtlCapability::SupportedNative`].
pub fn ttl_capability() -> BackendTtlCapability {
    BackendTtlCapability::SupportedNative
}

/// Apply schema TTL: no Redis DDL; expiry is set per key on create via [`expire_doc_key`].
pub fn apply_ttl_policy(_table: &str, _seconds: u64) -> Result<()> {
    Ok(())
}

/// `EXPIRE` on the document key when the table has a schema TTL policy.
pub async fn expire_doc_key(
    conn: &mut ConnectionManager,
    keys: &Keyspace,
    table: &str,
    id: &str,
) -> Result<()> {
    let Some(policy) = policy_for_table(table) else {
        return Ok(());
    };
    let secs = i64::try_from(policy.seconds).unwrap_or(i64::MAX);
    if secs <= 0 {
        return Ok(());
    }
    let doc_key = keys.doc(table, id);
    let _: bool = conn
        .expire(&doc_key, secs)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

/// `EXPIRE` matching unique-index keys for `fields` present as strings on `record`.
pub async fn expire_uniq_keys(
    conn: &mut ConnectionManager,
    keys: &Keyspace,
    table: &str,
    record: &serde_json::Value,
    fields: &[String],
) -> Result<()> {
    let Some(policy) = policy_for_table(table) else {
        return Ok(());
    };
    let secs = i64::try_from(policy.seconds).unwrap_or(i64::MAX);
    if secs <= 0 {
        return Ok(());
    }
    for field in fields {
        let Some(value) = record.get(field).and_then(|v| v.as_str()) else {
            continue;
        };
        let uniq = keys.uniq(table, field, value);
        let _: bool = conn
            .expire(&uniq, secs)
            .await
            .map_err(|e| Error::database(e.to_string()))?;
    }
    Ok(())
}

/// Remove `id` from the table membership set when the document key is gone.
pub async fn srem_orphan_id(
    conn: &mut ConnectionManager,
    keys: &Keyspace,
    table: &str,
    id: &str,
) -> Result<()> {
    let ids_key = keys.table_ids(table);
    let _: () = conn
        .srem(&ids_key, id)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_is_native() {
        assert_eq!(ttl_capability(), BackendTtlCapability::SupportedNative);
    }
}
