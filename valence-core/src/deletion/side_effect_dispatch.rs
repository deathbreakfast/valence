//! Type-erased delete side-effect dispatch for the queued/cascade physical-delete path.
//!
//! Integrators do **not** call this. Codegen registers [`DeleteSideEffectDescriptor`] via
//! `inventory`; the valence-platform deletion step worker invokes
//! [`dispatch_queued_delete_side_effects`] after a successful CascadeDelete.

use crate::runtime::Valence;
use std::future::Future;
use std::pin::Pin;

/// Type-erased delete side-effect runner (before-row as JSON).
pub type DeleteSideEffectDispatchFn =
    fn(Valence, serde_json::Value) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// One registered delete side-effect dispatcher for a table (submitted by codegen).
#[derive(Copy, Clone)]
pub struct DeleteSideEffectDescriptor {
    /// Physical table name from the schema DSL.
    pub table_name: &'static str,
    /// Deserializes `before` JSON and runs typed `dispatch_side_effects` for Delete.
    pub dispatch: DeleteSideEffectDispatchFn,
}

inventory::collect!(DeleteSideEffectDescriptor);

/// All registered delete side-effect descriptors.
#[must_use]
pub fn delete_side_effect_descriptors() -> Vec<&'static DeleteSideEffectDescriptor> {
    inventory::iter::<DeleteSideEffectDescriptor>
        .into_iter()
        .collect()
}

/// Find the delete side-effect descriptor for `table_name`, if any.
#[must_use]
pub fn find_delete_side_effect_descriptor(
    table_name: &str,
) -> Option<&'static DeleteSideEffectDescriptor> {
    delete_side_effect_descriptors()
        .into_iter()
        .find(|d| d.table_name == table_name)
}

/// Run registered Delete side effects for `table` using a pre-delete JSON snapshot.
///
/// No-op when the table has no `side_effects:` registration. Handler errors are logged inside
/// generated `dispatch_side_effects` and do not surface here.
pub async fn dispatch_queued_delete_side_effects(
    table: &str,
    before_json: serde_json::Value,
    valence: &Valence,
) {
    let Some(desc) = find_delete_side_effect_descriptor(table) else {
        return;
    };
    (desc.dispatch)(valence.clone(), before_json).await;
}
