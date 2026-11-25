//! `OwnershipService::list_owner_record_ids` — newest-first paging on the mem backend.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::owner_ref::{OwnerKind, OwnerRef};
use valence_core::{DatabaseBackend, OwnershipService, Valence};

fn mem_valence() -> (Valence, Arc<dyn DatabaseBackend>) {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    let v = Valence::builder()
        .add_backend("default", backend.clone())
        .with_actor(Actor::User {
            user_id: "u1".into(),
        })
        .build()
        .unwrap();
    (v, backend)
}

fn user(id: &str) -> OwnerRef {
    OwnerRef {
        owner_id: id.to_string(),
        owner_kind: OwnerKind::User,
    }
}

async fn own(model: &str, record: &str, owner: &str, v: &Valence) {
    OwnershipService::ensure_active_ownership(model, record, user(owner), v)
        .await
        .unwrap();
    // `created_at` is unix milliseconds; keep writes in distinct milliseconds.
    tokio::time::sleep(Duration::from_millis(3)).await;
}

async fn list(owner: &str, model: &str, limit: u32, offset: u32, v: &Valence) -> Vec<String> {
    OwnershipService::list_owner_record_ids(owner, "user", model, "active", limit, offset, v)
        .await
        .unwrap()
}

#[tokio::test]
async fn lists_newest_first_and_pages_happy() {
    let (v, _) = mem_valence();
    for r in ["r1", "r2", "r3", "r4"] {
        own("note", r, "alice", &v).await;
    }
    own("note", "other", "bob", &v).await;
    own("task", "t1", "alice", &v).await;

    assert_eq!(
        list("alice", "note", 10, 0, &v).await,
        ["r4", "r3", "r2", "r1"]
    );
    assert_eq!(list("alice", "note", 2, 0, &v).await, ["r4", "r3"]);
    assert_eq!(list("alice", "note", 2, 2, &v).await, ["r2", "r1"]);
    assert_eq!(list("alice", "note", 2, 4, &v).await, Vec::<String>::new());
    // Session ids with the `user:` prefix match the bare stored id.
    assert_eq!(list("user:alice", "note", 1, 0, &v).await, ["r4"]);
}

#[tokio::test]
async fn filters_by_status_sad() {
    let (v, _) = mem_valence();
    own("note", "r1", "alice", &v).await;
    let pending = OwnershipService::list_owner_record_ids(
        "alice",
        "user",
        "note",
        "pending_deletion",
        10,
        0,
        &v,
    )
    .await
    .unwrap();
    assert_eq!(pending, Vec::<String>::new());
}

#[tokio::test]
async fn rows_without_created_at_sort_last_sad() {
    let (v, backend) = mem_valence();
    let legacy_id = "legacy-row-id";
    backend
        .upsert_record(
            "valence_data_ownership",
            legacy_id,
            json!({
                "id": legacy_id,
                "valence_model": "note",
                "record_id": "legacy",
                "owner_id": "alice",
                "owner_type": "user",
                "status": "active",
            }),
        )
        .await
        .unwrap();
    own("note", "new", "alice", &v).await;

    assert_eq!(list("alice", "note", 10, 0, &v).await, ["new", "legacy"]);
}
