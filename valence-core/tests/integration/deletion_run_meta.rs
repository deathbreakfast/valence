//! `DeletionService::create_run_with_request` stores why a run was asked for and its due date.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::admin_entity_delete::{DeletionRequestMeta, DELETION_DUE_DAYS};
use valence_core::deletion::DeletionService;
use valence_core::{DatabaseBackend, Valence};

fn mem_valence() -> Valence {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    Valence::builder()
        .add_backend("default", backend)
        .with_actor(Actor::User {
            user_id: "admin1".into(),
        })
        .build()
        .expect("build")
}

fn actor_json() -> Value {
    serde_json::to_value(Actor::User {
        user_id: "admin1".into(),
    })
    .unwrap()
}

fn ts(run: &Value, key: &str) -> DateTime<Utc> {
    serde_json::from_value(run.get(key).cloned().unwrap_or(Value::Null))
        .unwrap_or_else(|e| panic!("{key} is not a timestamp: {e}; run = {run}"))
}

#[tokio::test]
async fn run_with_request_stores_reason_ticket_and_due_date() {
    let v = mem_valence();
    let meta = DeletionRequestMeta {
        reason: Some("User asked to close their account".into()),
        ticket: Some("SUP-1234".into()),
    };
    let run_id = DeletionService::create_run_with_request("widget", "w1", actor_json(), &meta, &v)
        .await
        .unwrap();
    let run = DeletionService::get_run_json(&run_id, &v)
        .await
        .unwrap()
        .expect("run row");
    assert_eq!(run["reason"], json!("User asked to close their account"));
    assert_eq!(run["ticket"], json!("SUP-1234"));
    let due = ts(&run, "due_at") - ts(&run, "requested_at");
    assert_eq!(due, chrono::Duration::days(DELETION_DUE_DAYS));
    assert_eq!(DELETION_DUE_DAYS, 30);
}

#[tokio::test]
async fn old_create_run_stores_no_reason() {
    let v = mem_valence();
    let run_id = DeletionService::create_run("widget", "w2", actor_json(), &v)
        .await
        .unwrap();
    let run = DeletionService::get_run_json(&run_id, &v)
        .await
        .unwrap()
        .expect("run row");
    assert!(run.get("reason").is_none_or(Value::is_null), "run = {run}");
    assert!(run.get("ticket").is_none_or(Value::is_null), "run = {run}");
    let due = ts(&run, "due_at") - ts(&run, "requested_at");
    assert_eq!(due, chrono::Duration::days(DELETION_DUE_DAYS));
}
