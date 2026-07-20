//! Instrumentation hooks for Valence runtime metrics and events.

mod backend;
pub(crate) mod deletion;
mod events;
mod labels;
pub(crate) mod metrics;
pub(crate) mod privacy;
pub(crate) mod query;
pub(crate) mod timing;

pub use backend::{wrap_backend, InstrumentedBackend};
pub use deletion::{record_dag_computed, record_restrict_blocked, record_run_queued};
pub use events::{error_log_fields, slow_op_fields};
pub use labels::{EdgeWriteOp, ReadOp, WriteOp};
pub use metrics::{
    maybe_record_slow_op, record_db_error, record_db_wall_ms, record_edge_read, record_edge_write,
    record_mutation_wall_ms, record_ownership_fetch_mode, record_read, record_retry_error,
    record_router_resolve_error, record_unique_violation, record_validation_failure, record_write,
};
pub use timing::MutationTimer;

/// Record ownership transfer audit (security).
pub fn record_ownership_transfer(
    table: &str,
    record_id: &str,
    from_owner_id: &str,
    from_owner_type: &str,
    to_owner_id: &str,
    to_owner_type: &str,
    actor: &str,
) {
    let _ = (
        record_id,
        from_owner_id,
        from_owner_type,
        to_owner_id,
        to_owner_type,
        actor,
    );
    metrics::record_ownership_transfer(table);
}

/// Side-effect dispatch counter (codegen `dispatch_side_effects` hook).
pub fn record_side_effect_dispatch(_table: &str, _kind: &str) {}

/// Side-effect handler failure hook (codegen `dispatch_side_effects`).
pub fn record_side_effect_error(_table: &str, _message: &str) {}
