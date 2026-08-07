//! MongoDB wire [`DatabaseBackend`](valence_core::DatabaseBackend) adapter for Valence.
//!
//! # TTL
//!
//! Implements **native** schema TTL (`SupportedNative`): idempotent TTL index on
//! [`valence_core::ttl::EXPIRE_AT_FIELD`] (`expireAfterSeconds: 0`) plus create-time stamps.
//! Call [`valence_core::Valence::ensure_ttl_for_all`] at boot; see [`valence_core::ttl`].

#![deny(missing_docs)]

mod backend;
mod config;
mod ttl;

pub use backend::{MongoBackend, ENGINE_ID, PRIMARY};
pub use config::{MongoBackendBuilder, MongoConfig, DATABASE_ENV, TEST_URI_ENV, URI_ENV};
