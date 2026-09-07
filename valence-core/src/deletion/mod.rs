//! Cascading delete orchestration — dispatch hooks, DAG planning, and sync delete-now.

mod apply;
mod dag_privacy;
mod dispatch;
mod execute;
mod prepare;
mod service;
mod side_effect_dispatch;

pub mod dag;

pub use apply::apply_deletion_node;
pub use dag_privacy::{check_dag_delete_privacy, check_dag_delete_privacy_with_registry};
pub use dispatch::{
    dispatch, is_deletion_dispatcher_registered, register_deletion_dispatcher,
    register_noop_deletion_dispatcher_for_tests, DeletionRequest,
};
pub use execute::{apply_deletion_dag, delete_entity_now};
pub use prepare::{
    normalize_record_id_for_deletion, prepare_deletion, DeletionMode, PreparedDeletion,
};
pub use service::DeletionService;
#[doc(hidden)]
pub use side_effect_dispatch::{
    dispatch_queued_delete_side_effects, DeleteSideEffectDescriptor, DeleteSideEffectDispatchFn,
};
