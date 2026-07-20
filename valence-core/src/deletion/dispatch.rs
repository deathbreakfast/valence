//! Optional dispatcher so the host can wire background workers without a core → job-runner dependency.

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

use crate::error::{Error, Result};

/// Payload passed to the registered deletion dispatcher (typically starts a host job orchestrator).
#[derive(Debug, Clone)]
pub struct DeletionRequest {
    pub run_id: String,
    pub root_table: String,
    pub root_record_id: String,
    pub actor_json: serde_json::Value,
}

type DispatchFn =
    dyn Fn(DeletionRequest) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync;

static DISPATCHER: OnceLock<Box<DispatchFn>> = OnceLock::new();

/// True if [`register_deletion_dispatcher`] (or a test dispatcher) was installed in this process.
pub fn is_deletion_dispatcher_registered() -> bool {
    DISPATCHER.get().is_some()
}

/// Register the process-wide deletion dispatcher (call once from server bootstrap).
pub fn register_deletion_dispatcher(f: Box<DispatchFn>) {
    let _ = DISPATCHER.set(f);
}

/// Install a no-op dispatcher when the slot is still empty (integration tests, harness crates).
///
/// `Model::delete` on app tables calls [`dispatch`]; without a host dispatcher this satisfies the
/// hook so deletes complete. If a real dispatcher was already registered, this is a no-op.
pub fn register_noop_deletion_dispatcher_for_tests() {
    let noop: Box<DispatchFn> = Box::new(|_| Box::pin(async move { Ok(()) }));
    let _ = DISPATCHER.set(noop);
}

/// Invoke the dispatcher if configured.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn dispatch(req: DeletionRequest) -> Result<()> {
    match DISPATCHER.get() {
        Some(f) => (f)(req).await,
        None => Err(Error::Internal(
            "Deletion dispatcher not registered; register a host dispatcher from server bootstrap"
                .into(),
        )),
    }
}
