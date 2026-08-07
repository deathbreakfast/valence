//! Table-level TTL: schema policy, create-time stamping, and ensure APIs.
//!
//! # Owns
//!
//! - [`SchemaTtlPolicy`] / [`BackendTtlCapability`]
//! - Reserved [`EXPIRE_AT_FIELD`] and [`prepare_create_content`]
//! - Ensure helpers used by [`crate::Valence::ensure_ttl_for_all`] /
//!   [`crate::Valence::ensure_ttl_for_table`]
//!
//! # Does not own
//!
//! Periodic deletion of Deferred rows (`valence_platform::ttl_sweep` Chronon sweeper —
//! host must call `register_ttl_service`).
//! Sliding TTL, field-level TTL, and Spectra TTL metrics.
//!
//! # Concern → API
//!
//! | Concern | API |
//! |---------|-----|
//! | Declare TTL on a table | `ttl:` in `valence_schema!` → [`SchemaTtlPolicy`] on `Schema.ttl` |
//! | Boot: apply or warn for every TTL schema | [`crate::Valence::ensure_ttl_for_all`] |
//! | Incremental single-table ensure | [`crate::Valence::ensure_ttl_for_table`] |
//! | Stamp expire on create | [`prepare_create_content`] / [`EXPIRE_AT_FIELD`] |
//! | Backend native apply | [`crate::DatabaseBackend::ttl_capability`] / `apply_ttl_policy` |
//!
//! # Examples
//!
//! Hosts wire TTL once after backends are registered — do not maintain a hand list of tables:
//!
//! ```ignore
//! use std::sync::Arc;
//! use valence::prelude::*;
//! use valence::{
//!     valence_schema, Database, DatabaseFromEngine, FieldType, RedisBackend, Valence,
//!     REDIS_ENGINE_ID,
//! };
//!
//! const SESSION_DB: DatabaseFromEngine =
//!     Database::from_engine("default", REDIS_ENGINE_ID);
//!
//! valence_schema! {
//!     SessionToken {
//!         table: "session_token",
//!         version: "0.1.0",
//!         database: SESSION_DB,
//!         ttl: { seconds: 1800 },
//!         fields: [
//!             id: { r#type: FieldType::String, primary_key: true, required: true },
//!             subject: { r#type: FieldType::String, required: true },
//!         ],
//!     }
//! }
//!
//! async fn boot() -> valence::Result<()> {
//!     let backend = RedisBackend::from_env().await?;
//!     let valence = Valence::builder()
//!         .add_backend("default", Arc::new(backend))
//!         .build()?;
//!     valence.ensure_ttl_for_all().await?;
//!     Ok(())
//! }
//! ```
//!
//! Prefer [`crate::DatabaseBackend`] TTL methods over the unused [`BackendTtlAdapter`] trait.

mod ensure;
mod policy;
mod stamp;

pub use ensure::{
    ensure_ttl_for_all, ensure_ttl_for_table, list_ttl_table_names, reset_ttl_warn_state_for_tests,
    ttl_warn_emit_count_for_tests,
};
pub use policy::{BackendTtlAdapter, BackendTtlCapability, SchemaTtlPolicy};
pub use stamp::{
    policy_for_table, prepare_create_content, prepare_create_content_with_capability,
    stamp_expire_at_if_absent, EXPIRE_AT_FIELD,
};
