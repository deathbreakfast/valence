//! TM-V2 / TM-V4 — queued delete side-effect dispatch via inventory.

use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::deletion::{dispatch_queued_delete_side_effects, DeleteSideEffectDescriptor};
use valence_core::Valence;

static SE_CALLS: AtomicUsize = AtomicUsize::new(0);
static SE_ERR_CALLS: AtomicUsize = AtomicUsize::new(0);

fn test_table_dispatch(
    _v: valence_core::Valence,
    _row: serde_json::Value,
) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        SE_CALLS.fetch_add(1, Ordering::SeqCst);
    })
}

/// Mirrors codegen: handler `Err` is swallowed; dispatch returns `()`.
fn erring_table_dispatch(
    _v: valence_core::Valence,
    _row: serde_json::Value,
) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        SE_ERR_CALLS.fetch_add(1, Ordering::SeqCst);
        // Simulated SideEffect Err path: log-only; do not panic or return Err.
    })
}

valence_core::inventory::submit! {
    DeleteSideEffectDescriptor {
        table_name: "se_dispatch_probe",
        dispatch: test_table_dispatch,
    }
}

valence_core::inventory::submit! {
    DeleteSideEffectDescriptor {
        table_name: "se_dispatch_err_probe",
        dispatch: erring_table_dispatch,
    }
}

#[tokio::test]
async fn tm_v2_dispatch_runs_registered_delete_side_effect() {
    SE_CALLS.store(0, Ordering::SeqCst);
    let v = Valence::builder()
        .add_backend("default", Arc::new(InMemoryBackend::new()))
        .with_actor(Actor::User {
            user_id: "u1".into(),
        })
        .build()
        .unwrap();

    dispatch_queued_delete_side_effects("se_dispatch_probe", json!({"id": "r1", "name": "x"}), &v)
        .await;
    assert_eq!(SE_CALLS.load(Ordering::SeqCst), 1);

    // Unknown table: no-op
    dispatch_queued_delete_side_effects("no_such_table", json!({}), &v).await;
    assert_eq!(SE_CALLS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn tm_v4_side_effect_error_does_not_fail_dispatch() {
    SE_ERR_CALLS.store(0, Ordering::SeqCst);
    let v = Valence::builder()
        .add_backend("default", Arc::new(InMemoryBackend::new()))
        .with_actor(Actor::User {
            user_id: "u1".into(),
        })
        .build()
        .unwrap();

    // Completes successfully even when the registered handler "fails" internally.
    dispatch_queued_delete_side_effects("se_dispatch_err_probe", json!({"id": "r1"}), &v).await;
    assert_eq!(SE_ERR_CALLS.load(Ordering::SeqCst), 1);
}
