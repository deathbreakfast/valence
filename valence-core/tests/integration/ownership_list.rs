//! `OwnershipService::list_owner_record_ids` — newest-first paging on the mem backend.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

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

/// Write an active ownership row with an explicit `created_at` so order never depends on
/// the clock.
async fn own_at(
    model: &str,
    record: &str,
    owner: &str,
    created_at: i64,
    backend: &Arc<dyn DatabaseBackend>,
) {
    let id = format!("{model}-{record}");
    backend
        .upsert_record(
            "valence_data_ownership",
            &id,
            json!({
                "id": id,
                "valence_model": model,
                "record_id": record,
                "owner_id": owner,
                "owner_type": "user",
                "status": "active",
                "created_at": created_at,
            }),
        )
        .await
        .unwrap();
}

async fn list(owner: &str, model: &str, limit: u32, offset: u32, v: &Valence) -> Vec<String> {
    OwnershipService::list_owner_record_ids(owner, "user", model, "active", limit, offset, v)
        .await
        .unwrap()
}

#[tokio::test]
async fn lists_newest_first_and_pages_happy() {
    let (v, backend) = mem_valence();
    // Inserted out of order so the result can't be insertion order.
    for (r, at) in [("r2", 200), ("r4", 400), ("r1", 100), ("r3", 300)] {
        own_at("note", r, "alice", at, &backend).await;
    }
    own_at("note", "other", "bob", 500, &backend).await;
    own_at("task", "t1", "alice", 600, &backend).await;

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
async fn equal_created_at_breaks_ties_by_id_descending_happy() {
    let (v, backend) = mem_valence();
    own_at("note", "a", "alice", 100, &backend).await;
    own_at("note", "b", "alice", 100, &backend).await;
    assert_eq!(list("alice", "note", 10, 0, &v).await, ["b", "a"]);
}

#[tokio::test]
async fn filters_by_status_sad() {
    let (v, backend) = mem_valence();
    own_at("note", "r1", "alice", 100, &backend).await;
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
    backend
        .upsert_record(
            "valence_data_ownership",
            "legacy-row-id",
            json!({
                "id": "legacy-row-id",
                "valence_model": "note",
                "record_id": "legacy",
                "owner_id": "alice",
                "owner_type": "user",
                "status": "active",
            }),
        )
        .await
        .unwrap();
    own_at("note", "old", "alice", 1, &backend).await;
    own_at("note", "new", "alice", 2, &backend).await;

    assert_eq!(
        list("alice", "note", 10, 0, &v).await,
        ["new", "old", "legacy"]
    );
}

#[tokio::test]
async fn ensure_active_ownership_stamps_created_at_happy() {
    let (v, _) = mem_valence();
    OwnershipService::ensure_active_ownership("note", "r1", user("alice"), &v)
        .await
        .unwrap();
    let row = OwnershipService::get_ownership_json("note", "r1", &v)
        .await
        .unwrap()
        .unwrap();
    assert!(row["created_at"].as_i64().is_some_and(|at| at > 0), "{row}");
}
