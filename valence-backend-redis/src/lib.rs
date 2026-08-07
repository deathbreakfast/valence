//! Redis wire [`DatabaseBackend`](valence_core::DatabaseBackend) adapter for Valence.
//!
//! # TTL
//!
//! Implements **native** schema TTL (`SupportedNative`): create-time `EXPIRE` on document
//! and unique-index keys. Call [`valence_core::Valence::ensure_ttl_for_all`] at boot; see
//! [`valence_core::ttl`].

#![deny(missing_docs)]

mod backend;
mod config;
mod fleet;
mod keys;
mod ttl;

pub use backend::{RedisBackend, ENGINE_ID, PRIMARY};
pub use config::{
    test_url, FleetRedisBackendBuilder, RedisBackendBuilder, RedisConfig, KEY_PREFIX_ENV,
    TEST_URL_ENV, URLS_ENV, URL_ENV,
};
pub use fleet::{connect_fleet_arc, FleetRedisBackend};
