//! Valence core ports: storage routing, host-injectable traits, and runtime builder.
//!
//! ## Stack position
//!
//! ```text
//! valence public crate → valence-core (this crate) → valence-telemetry
//! backends (mem, surreal, third-party) implement [`DatabaseBackend`]
//! ```
//!
//! ## Entry points
//!
//! - [`Valence`] / [`ValenceBuilder`] — boot and wire backends
//! - [`DatabaseBackend`] — storage adapter trait (open `engine_id`)
//! - [`DatabaseRouter`] — heterogeneous engine registry
//! - [`ports`] — secrets, actor, endpoints (host-injectable)
//! - [`Model`] — generated CRUD surface
//! - [`ttl`] — schema TTL policy, [`prepare_create_content`], ensure via
//!   [`Valence::ensure_ttl_for_all`] / [`Valence::ensure_ttl_for_table`]
//!
//! ## Examples
//!
//! ```
//! use std::sync::Arc;
//! use valence_backend_mem::InMemoryBackend;
//! use valence_core::Valence;
//!
//! let valence = Valence::builder()
//!     .add_backend("default", Arc::new(InMemoryBackend::new()))
//!     .build()
//!     .expect("build");
//! assert!(valence.active_backend().is_ok());
//! ```
//!
//! ## Gotchas
//!
//! - Engine SDKs and product host crates must never appear in this crate
//!   (use `valence-backend-*` and separate host adapters)
//! - Host-owned codegen lives in `valence-codegen`, not here
//! - Table TTL is create-only; Deferred backends need the platform sweeper (Future)

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

extern crate self as valence_core;

pub mod actor;
pub mod actor_policy;
pub mod admin_entity_delete;
pub mod backend;
pub mod batch;
pub mod compiled_query;
pub mod compiled_query_factory;
pub mod connection;
pub mod currency;
pub mod database_retry;
pub mod datetime_unix;
pub mod deletion;
pub mod entity;
pub mod error;
pub mod evaluator;
pub mod iter;
pub mod json_as;
pub mod known_engines;
pub mod model;
pub mod owner_ref;
pub mod ownership;
pub mod ports;
pub mod privacy;
pub mod privacy_policies;
pub mod query;
pub mod query_compiler;
pub mod query_compiler_registry;
pub mod read_cache;
pub mod record_id;
pub mod redact;
pub mod reference;
pub mod register_logical;
pub mod registry;
pub mod request_cache;
pub mod router;
pub mod router_key;
pub mod row_json;
pub mod runtime;
pub mod safe_ident;
pub mod schema;
pub mod schema_api;
pub mod schema_types;
pub mod side_effect;
pub mod trait_registry;
pub mod trait_schema;
pub mod ttl;
pub mod validation;

#[cfg(feature = "instrumentation")]
pub mod instrumentation;

/// Hidden re-exports for generated model code and platform migrations.
#[doc(hidden)]
pub mod __internal {
    pub use crate::compiled_query::CompiledQuery;
    pub use crate::query_compiler::QueryCompiler;
}

pub use actor::Actor;
pub use actor_policy::{
    is_system_shaped_actor, ActorJsonPolicy, ActorTrust, RejectExternalSystemActor,
};
pub use admin_entity_delete::{queue_delete_entity, queue_delete_entity_returning_run_id};
#[cfg(feature = "compiler-indradb")]
pub use backend::IndraQueryCompiler;
#[cfg(feature = "compiler-mongodb")]
pub use backend::MongoQueryCompiler;
#[cfg(feature = "compiler-redis")]
pub use backend::RedisQueryCompiler;
#[cfg(feature = "compiler-sql")]
pub use backend::SqlQueryCompiler;
#[cfg(feature = "compiler-surreal")]
pub use backend::SurrealQueryCompiler;
pub use backend::{BackendCapabilities, DatabaseBackend};
pub use batch::BatchCreatable;
pub use compiled_query::CompiledQuery;
pub use connection::{
    extract_id_from_record, extract_id_from_record_display, extract_id_from_select_value,
    id_from_model, Cardinality, IdHolder, OnDelete,
};
pub use currency::{Currency, CurrencyCode, CurrencyError, ParseCurrencyCodeError};
pub use database_retry::retry_on_database_tx_conflict;
pub use deletion::{
    apply_deletion_node, check_dag_delete_privacy, check_dag_delete_privacy_with_registry,
    dispatch, is_deletion_dispatcher_registered, register_deletion_dispatcher,
    register_noop_deletion_dispatcher_for_tests, DeletionRequest, DeletionService,
};
#[doc(hidden)]
pub use deletion::{
    dispatch_queued_delete_side_effects, DeleteSideEffectDescriptor, DeleteSideEffectDispatchFn,
};
pub use entity::ValenceEntity;
pub use error::{Error, Result};
pub use evaluator::{
    Database, DatabaseEvaluator, DatabaseFromEngine, ResolverContext, DEFAULT_IN_MEMORY,
    DEFAULT_IN_MEMORY_ROUTER_KEY,
};
pub use iter::{
    find_iter_descriptor, iter_descriptors_for_table, IterDescriptor, IterEvaluation,
    IterExecuteFn, IterShouldRunFn,
};
pub use known_engines::KnownEngines;
pub use model::{FieldOperation, Model, PrivacyError, SchemaMetadata};
pub use owner_ref::{OwnerKind, OwnerRef, OwnershipConfig};
pub use ownership::OwnershipService;
pub use ports::actor::{ActorContext, ActorFactory, JsonActorContext, JsonActorFactory};
pub use ports::endpoints::{
    DatabaseEndpointResolver, EnvEndpointResolver, NoopEndpointResolver, StaticEndpointResolver,
};
pub use ports::secrets::{EnvSecretProvider, NoOpSecretProvider, SecretProvider};
pub use privacy::{
    privacy_bypass_active, PolicyEvaluator, PrivacyEvaluator, PrivacyOperation, PrivacyRule,
    PRIVACY_BYPASS_ENV, PRIVACY_BYPASS_FORCE_ON_ENV,
};
pub use query::{
    DateTimePredicate, HopSource, HopType, IdOnlyRecord, IntPredicate, NullPredicate, QueryCore,
    RecordPredicate, SortDirection, StringPredicate, MAX_QUERY_LIMIT, MAX_QUERY_OFFSET,
};
pub use query_compiler::QueryCompiler;
pub use query_compiler_registry::{
    compile_for_engine, global_compiler_registry, QueryCompilerRegistry,
};
pub use read_cache::{
    clear_for_test, get_record_via_cache, get_record_with_ownership_bundle_via_cache, invalidate,
    invalidate_for_backend, read_cache_enabled,
};
pub use record_id::RecordId;
pub use redact::{redact_credentials_in_text, redact_endpoint};
pub use reference::{Reference, ReferencedEntity, WithReference};
pub use register_logical::{
    register_backend_logical_names, register_backend_logical_names_slices,
    RegisterBackendLogicalNamesOptions,
};
pub use request_cache::RequestPermissionCache;
pub use router::DatabaseRouter;
pub use router_key::router_key;
pub use runtime::{
    RouterValenceFactory, RouterValenceFactoryConfig, Valence, ValenceBuilder, ValenceFactory,
};
pub use schema::{
    schema_connections_for_table, SchemaConnectionsOverlayInit, SchemaMetadataInit,
    SchemaMetadataStruct, SchemaRegistry,
};
pub use schema_api::{
    ForeignKeyRef, Schema, SchemaConnection, SchemaEdge, SchemaField, SchemaMeta, SchemaPolicies,
    SchemaPolicyRule, SchemaPolicyRules, SchemaPrivacy,
};
pub use schema_types::{FieldType, JsonAsSerdeError, Role, Validator};
pub use side_effect::{FieldChange, Mutation, MutationKind, SideEffect};
pub use trait_registry::TraitRegistry;
pub use trait_schema::{
    TraitDefinition, TraitDefinitionInit, TraitFieldDef, TraitImplementor, TraitPolicies,
    TraitPolicyRules,
};
pub use ttl::{
    list_ttl_table_names, prepare_create_content, reset_ttl_warn_state_for_tests,
    ttl_warn_emit_count_for_tests, BackendTtlCapability, SchemaTtlPolicy, EXPIRE_AT_FIELD,
};
pub use valence_telemetry::{ConsoleSink, NoOpSink, TelemetrySink};

#[cfg(feature = "instrumentation")]
pub use instrumentation::{wrap_backend, InstrumentedBackend, MutationTimer};

pub use inventory;
