//! `ModelBatch` (privacy-checked all-or-nothing writes) and `update_if`
//! (compare-and-set update), both under the session actor.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use codegen_host::Note;
use valence::{
    Actor, BatchCreatable, DatabaseBackend, Error, InMemoryBackend, Model, ModelBatch, RecordId,
    Valence,
};

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
            operation: "model_batch_lab".into(),
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

fn note(id: &str, user: &str, title: &str) -> Note {
    let mut n = Note::new(user.into(), title.into()).expect("new");
    n.set_id(RecordId::new("note", id));
    n
}

async fn seed(viewer: &Valence, id: &str, user: &str, title: &str) {
    Note::upsert(
        id,
        note(id, user, title),
        viewer,
        valence::use_!(r"In the **model batch** suite, we **save a note by id** so later batch and compare-and-set steps have a row to work on. Developers running that example suite use this row."),
    )
    .await
    .expect("seed note");
}

async fn queue_create(batch: &mut ModelBatch<'_>, n: Note) -> valence::Result<()> {
    Note::batch_create(
        n,
        batch,
        valence::use_!(r"In the **model batch** suite, we **queue a new note in a batch** so the suite can check all-or-nothing writes. Developers running that example suite use this row."),
    )
    .await
}

async fn queue_update(batch: &mut ModelBatch<'_>, n: Note) -> valence::Result<()> {
    let id = n.id().expect("id").id().to_string();
    Note::batch_update(
        &id,
        n,
        batch,
        valence::use_!(r"In the **model batch** suite, we **queue a note update in a batch** so the suite can check privacy on batched updates. Developers running that example suite use this row."),
    )
    .await
}

async fn update_if(viewer: &Valence, n: Note, expected_title: &str) -> valence::Result<Note> {
    let id = n.id().expect("id").id().to_string();
    Note::update_if(
        &id,
        n,
        "title",
        serde_json::json!(expected_title),
        viewer,
        valence::use_!(r"In the **compare-and-set** suite, we **rename a note only if its title is unchanged** so the suite can check conflict handling. Developers running that example suite use this row."),
    )
    .await
}

async fn stored(lab: &Lab, id: &str) -> Option<serde_json::Value> {
    lab.backend.get_record("note", id).await.expect("raw read")
}

#[tokio::test]
async fn batch_commits_creates_and_updates_together_happy() {
    let lab = lab();
    seed(&lab.alice, "n1", "alice", "draft").await;

    let mut batch = ModelBatch::new(&lab.alice);
    queue_create(&mut batch, note("n2", "alice", "second"))
        .await
        .expect("queue create");
    queue_update(&mut batch, note("n1", "alice", "final"))
        .await
        .expect("queue update");
    assert_eq!(batch.len(), 2);
    let rows = batch.commit().await.expect("commit");
    assert_eq!(rows.len(), 2);

    assert_eq!(stored(&lab, "n1").await.expect("n1")["title"], "final");
    assert_eq!(stored(&lab, "n2").await.expect("n2")["title"], "second");
}

#[tokio::test]
async fn batch_refuses_a_row_the_actor_cannot_update_sad() {
    let lab = lab();
    seed(&lab.alice, "n1", "alice", "groceries").await;

    let mut batch = ModelBatch::new(&lab.bob);
    queue_create(&mut batch, note("n2", "bob", "bob's own"))
        .await
        .expect("bob may create his own note");
    let err = queue_update(&mut batch, note("n1", "bob", "taken"))
        .await
        .expect_err("only alice may update n1");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
    drop(batch);

    assert!(stored(&lab, "n2").await.is_none(), "nothing was committed");
    assert_eq!(stored(&lab, "n1").await.expect("n1")["title"], "groceries");
}

#[tokio::test]
async fn batch_failing_op_rolls_back_every_write_sad() {
    let lab = lab();
    seed(&lab.alice, "n1", "alice", "draft").await;

    let mut batch = ModelBatch::new(&lab.alice);
    queue_create(&mut batch, note("n2", "alice", "second"))
        .await
        .expect("queue create");
    queue_update(&mut batch, note("n1", "alice", "final"))
        .await
        .expect("queue update");
    // The row disappears between queueing and commit, so the update op fails.
    lab.backend
        .delete_record("note", "n1")
        .await
        .expect("delete");

    let err = batch.commit().await.expect_err("update target is gone");
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    assert!(stored(&lab, "n2").await.is_none(), "create rolled back");
}

#[tokio::test]
async fn batch_update_of_missing_row_is_not_found_sad() {
    let lab = lab();
    let mut batch = ModelBatch::new(&lab.alice);
    let err = queue_update(&mut batch, note("nope", "alice", "x"))
        .await
        .expect_err("no such row");
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    assert!(batch.is_empty());
}

#[tokio::test]
async fn update_if_writes_only_while_the_guard_matches() {
    let lab = lab();
    seed(&lab.alice, "n1", "alice", "v1").await;

    let updated = update_if(&lab.alice, note("n1", "alice", "v2"), "v1")
        .await
        .expect("guard matches");
    assert_eq!(updated.title(), "v2");

    let err = update_if(&lab.alice, note("n1", "alice", "stale"), "v1")
        .await
        .expect_err("title moved on to v2");
    assert!(matches!(err, Error::Conflict(_)), "{err:?}");
    assert_eq!(stored(&lab, "n1").await.expect("n1")["title"], "v2");
}

#[tokio::test]
async fn update_if_checks_update_privacy_on_the_stored_row_sad() {
    let lab = lab();
    seed(&lab.alice, "n1", "alice", "v1").await;

    let err = update_if(&lab.bob, note("n1", "bob", "taken"), "v1")
        .await
        .expect_err("only alice may update n1");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
    assert_eq!(stored(&lab, "n1").await.expect("n1")["title"], "v1");

    let err = update_if(&lab.alice, note("nope", "alice", "x"), "v1")
        .await
        .expect_err("no such row");
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
}
