//! Shared deletion dispatcher capture for harness integration tests.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use valence_core::deletion::{register_deletion_dispatcher, DeletionRequest};
use valence_core::Result;

static CAPTURE: OnceLock<Arc<Mutex<Vec<DeletionRequest>>>> = OnceLock::new();

fn captured_requests() -> Arc<Mutex<Vec<DeletionRequest>>> {
    Arc::clone(CAPTURE.get_or_init(|| {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let hook_target = Arc::clone(&captured);
        let dispatcher: Box<
            dyn Fn(DeletionRequest) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
                + Send
                + Sync,
        > = Box::new(move |req| {
            let hook_target = Arc::clone(&hook_target);
            Box::pin(async move {
                hook_target
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(req);
                Ok(())
            })
        });
        register_deletion_dispatcher(dispatcher);
        captured
    }))
}

/// Return the process-wide captured deletion requests and clear prior entries.
pub fn reset_deletion_capture() -> Arc<Mutex<Vec<DeletionRequest>>> {
    let captured = captured_requests();
    captured
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    captured
}
