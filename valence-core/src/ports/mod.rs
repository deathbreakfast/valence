//! Host-injectable ports (secrets, identity, endpoints).
//!
//! This crate defines each **trait** plus a minimal reference impl. Product integrations
//! ship as separate crates and are wired on [`crate::ValenceBuilder`] at boot.
//!
//! | Port | Trait | Reference impl | Builder |
//! |------|-------|----------------|---------|
//! | Storage | [`crate::DatabaseBackend`] | `valence-backend-mem` | [`.add_backend(...)`](crate::ValenceBuilder::add_backend) |
//! | Telemetry | [`TelemetrySink`](valence_telemetry::TelemetrySink) | [`NoOpSink`](valence_telemetry::NoOpSink), [`ConsoleSink`](valence_telemetry::ConsoleSink) | [`.telemetry_sink(...)`](crate::ValenceBuilder::telemetry_sink) |
//! | Secrets | [`SecretProvider`](secrets::SecretProvider) | [`NoOpSecretProvider`](secrets::NoOpSecretProvider), [`EnvSecretProvider`](secrets::EnvSecretProvider) | [`.secret_provider(...)`](crate::ValenceBuilder::secret_provider) |
//! | Identity | [`ActorFactory`](actor::ActorFactory) | [`JsonActorFactory`](actor::JsonActorFactory) | [`.actor_factory(...)`](crate::ValenceBuilder::actor_factory) |
//! | Endpoints | [`DatabaseEndpointResolver`](endpoints::DatabaseEndpointResolver) | [`NoopEndpointResolver`](endpoints::NoopEndpointResolver), [`EnvEndpointResolver`](endpoints::EnvEndpointResolver) | [`.endpoint_resolver(...)`](crate::ValenceBuilder::endpoint_resolver) |
//! | Extensions | any `Send + Sync` type | none (empty by default) | [`.extension(...)`](crate::ValenceBuilder::extension) |
//! | Deletion | host registration hook | no-op default | [`crate::deletion::register_deletion_dispatcher`] |
//!
//! Ports that only a host's privacy evaluators consult (a bot gate, a rate limiter) have no
//! trait here. Register them by type with [`crate::ValenceBuilder::extension`] and read them
//! back inside [`crate::PolicyEvaluator::evaluate`] with [`crate::Valence::extension`], so each
//! `Valence` carries its own instead of the host keeping a process-global.
//!
//! Privacy rules are **not** builder ports — attach [`crate::PolicyEvaluator`] consts in
//! `valence_schema!` (see [`crate::privacy`]).
//!
//! # Examples
//!
//! ```
//! use std::sync::Arc;
//! use valence_backend_mem::InMemoryBackend;
//! use valence_core::{EnvSecretProvider, JsonActorFactory, NoopEndpointResolver, Valence};
//! use valence_telemetry::ConsoleSink;
//!
//! let valence = Valence::builder()
//!     .add_backend("default", Arc::new(InMemoryBackend::new()))
//!     .secret_provider(Arc::new(EnvSecretProvider))
//!     .actor_factory(Arc::new(JsonActorFactory))
//!     .endpoint_resolver(Arc::new(NoopEndpointResolver))
//!     .telemetry_sink(Arc::new(ConsoleSink))
//!     .build()
//!     .expect("build");
//! assert!(valence.active_backend().is_ok());
//! ```

pub mod actor;
pub mod endpoints;
pub mod extensions;
pub mod secrets;
