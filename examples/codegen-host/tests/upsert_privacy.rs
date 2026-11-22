//! `Model::upsert` decides create vs update from storage, not from what the caller
//! can read.
//!
//! A row the caller can't read must not look absent: otherwise only create
//! policies run, and they check the caller's own payload, so a peer could
//! overwrite the owner's row by naming its id.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use codegen_host::{Diary, Note};
use valence::{Actor, DatabaseBackend, Error, InMemoryBackend, Model, Valence};

struct Lab {
    backend: Arc<InMemoryBackend>,
    alice: Valence,
    bob: Valence,
}

fn lab() -> Lab {
    let backend = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", Arc::clone(&backend) as Arc<dyn DatabaseBackend>)
        .with_actor(Actor::System {
            operation: "upsert_privacy_lab".into(),
        })
        .build()
        .expect("build");
    let alice = valence.with_actor(Actor::User {
        user_id: "alice".into(),
    });
    let bob = valence.with_actor(Actor::User {
        user_id: "bob".into(),
    });
    Lab {
        backend,
        alice,
        bob,
    }
}

async fn upsert_diary(
    viewer: &Valence,
    id: &str,
    user: &str,
    body: &str,
) -> valence::Result<Diary> {
    Diary::upsert(
        id,
        Diary::new(user.into(), body.into()).expect("new"),
        viewer,
        valence::use_!(r"In the **upsert privacy** suite, we **save a diary entry by id** so the suite can check who may create or replace it. Developers running that example suite use this row."),
    )
    .await
}

async fn upsert_note(viewer: &Valence, id: &str, user: &str, title: &str) -> valence::Result<Note> {
    Note::upsert(
        id,
        Note::new(user.into(), title.into()).expect("new"),
        viewer,
        valence::use_!(r"In the **upsert privacy** suite, we **save a note by id** so the suite can check who may replace it. Developers running that example suite use this row."),
    )
    .await
}

async fn stored(lab: &Lab, table: &str, id: &str) -> serde_json::Value {
    lab.backend
        .get_record(table, id)
        .await
        .expect("raw read")
        .expect("row exists")
}

#[tokio::test]
async fn peer_cannot_take_over_a_row_it_cannot_read() {
    let lab = lab();
    upsert_diary(&lab.alice, "d1", "alice", "dear diary")
        .await
        .expect("alice creates her entry");

    let err = upsert_diary(&lab.bob, "d1", "bob", "mine now")
        .await
        .expect_err("bob can't read alice's entry, so he can't upsert it");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");

    let row = stored(&lab, "diary", "d1").await;
    assert_eq!(row["user"], "alice");
    assert_eq!(row["body"], "dear diary");
}

#[tokio::test]
async fn update_policy_is_checked_on_the_stored_row() {
    let lab = lab();
    upsert_note(&lab.alice, "n1", "alice", "groceries")
        .await
        .expect("alice creates her note");

    // Bob can read the note but only its owner may update it. His payload names
    // himself as owner, which must not count.
    let err = upsert_note(&lab.bob, "n1", "bob", "taken")
        .await
        .expect_err("stored row belongs to alice");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");

    let row = stored(&lab, "note", "n1").await;
    assert_eq!(row["user"], "alice");
    assert_eq!(row["title"], "groceries");
}

#[tokio::test]
async fn owner_upsert_and_fresh_ids_still_work() {
    let lab = lab();
    upsert_diary(&lab.alice, "d1", "alice", "first")
        .await
        .expect("create");
    let replaced = upsert_diary(&lab.alice, "d1", "alice", "second")
        .await
        .expect("alice replaces her own entry");
    assert_eq!(replaced.body(), "second");

    upsert_diary(&lab.bob, "d2", "bob", "bob's own")
        .await
        .expect("bob creates his own entry under a new id");
    assert_eq!(stored(&lab, "diary", "d1").await["body"], "second");
    assert_eq!(stored(&lab, "diary", "d2").await["user"], "bob");
}
