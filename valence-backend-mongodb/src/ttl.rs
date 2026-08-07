//! MongoDB native TTL index helpers.

use mongodb::bson::doc;
use mongodb::bson::Document;
use mongodb::options::IndexOptions;
use mongodb::{Collection, IndexModel};

use valence_core::ttl::{BackendTtlCapability, EXPIRE_AT_FIELD};
use valence_core::{Error, Result};

/// Mongo reports [`BackendTtlCapability::SupportedNative`].
pub fn ttl_capability() -> BackendTtlCapability {
    BackendTtlCapability::SupportedNative
}

/// Idempotent TTL index on [`EXPIRE_AT_FIELD`] with `expireAfterSeconds: 0`.
pub async fn apply_ttl_policy(coll: &Collection<Document>) -> Result<()> {
    let index = IndexModel::builder()
        .keys(doc! { EXPIRE_AT_FIELD: 1 })
        .options(
            IndexOptions::builder()
                .expire_after(Some(std::time::Duration::from_secs(0)))
                .name(Some("valence_ttl_expire_at".to_string()))
                .build(),
        )
        .build();
    coll.create_index(index)
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
