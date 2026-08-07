//! Cascading delete orchestration — dispatch hooks and DAG planning.
//!
//! Hosts register a deletion dispatcher at boot; [`DeletionService`] coordinates graph expansion
//! via the [`dag`] submodule.
mod apply;
mod dag_privacy;
mod dispatch;
mod service;
mod side_effect_dispatch;

pub mod dag;

pub use apply::apply_deletion_node;
pub use dag_privacy::{check_dag_delete_privacy, check_dag_delete_privacy_with_registry};
pub use dispatch::{
    dispatch, is_deletion_dispatcher_registered, register_deletion_dispatcher,
    register_noop_deletion_dispatcher_for_tests, DeletionRequest,
};
pub use service::DeletionService;
#[doc(hidden)]
pub use side_effect_dispatch::{
    dispatch_queued_delete_side_effects, DeleteSideEffectDescriptor, DeleteSideEffectDispatchFn,
};
