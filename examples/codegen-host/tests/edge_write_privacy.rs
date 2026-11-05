//! `relate_to_*` / `unrelate_from_*` run the source record's update policy, so a
//! signed-in user who does not own the note cannot change its labels.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use codegen_host::{Label, Note};
use valence::{Actor, DatabaseBackend, Error, InMemoryBackend, Model, Valence};

struct Lab {
    system: Valence,
    alice: Valence,
    bob: Valence,
    note: Note,
    label: Label,
}

async fn lab() -> Lab {
    let backend = Arc::new(InMemoryBackend::new());
    let system = Valence::builder()
        .add_backend("default", backend as Arc<dyn DatabaseBackend>)
        .with_actor(Actor::System {
            operation: "edge_write_privacy_lab".into(),
        })
        .build()
        .expect("build");
    let alice = system.with_actor(Actor::User {
        user_id: "alice".into(),
    });
    let bob = system.with_actor(Actor::User {
        user_id: "bob".into(),
    });
    let note = Note::create(
        Note::new("alice".into(), "Groceries".into()).expect("new note"),
        &alice,
        valence::use_!(r"When the **edge-write privacy** suite starts, we **create alice's note** so later steps can label it as alice, bob and the system. Developers running that example suite use this row."),
    )
    .await
    .expect("alice creates her note");
    let label = Label::create(
        Label::new("errands".into()).expect("new label"),
        &alice,
        valence::use_!(r"When the **edge-write privacy** suite starts, we **create a label** so later steps can attach it to the note. Developers running that example suite use this row."),
    )
    .await
    .expect("create label");
    Lab {
        system,
        alice,
        bob,
        note,
        label,
    }
}

async fn labels_of(lab: &Lab) -> usize {
    lab.note
        .get_labels(
            &lab.system,
            valence::use_!(r"In the **edge-write privacy** suite, we **list the note's labels** so each step can check whether an edge write landed. Developers running that example suite use this result."),
        )
        .await
        .expect("get_labels")
        .len()
}

async fn relate(lab: &Lab, viewer: &Valence) -> valence::Result<()> {
    lab.note
        .relate_to_label(
            &lab.label,
            viewer,
            valence::use_!(r"In the **edge-write privacy** suite, we **attach the label to the note** so the suite can check who may change a note's labels. Developers running that example suite use this edge."),
        )
        .await
}

async fn unrelate(lab: &Lab, viewer: &Valence) -> valence::Result<()> {
    lab.note
        .unrelate_from_label(
            &lab.label,
            viewer,
            valence::use_!(r"In the **edge-write privacy** suite, we **detach the label from the note** so the suite can check who may change a note's labels. Developers running that example suite use this edge."),
        )
        .await
}

#[tokio::test]
async fn owner_and_system_may_relate_and_unrelate() {
    let lab = lab().await;
    relate(&lab, &lab.alice).await.expect("owner relates");
    assert_eq!(labels_of(&lab).await, 1);
    unrelate(&lab, &lab.alice).await.expect("owner unrelates");
    assert_eq!(labels_of(&lab).await, 0);
    relate(&lab, &lab.system).await.expect("system relates");
    assert_eq!(labels_of(&lab).await, 1);
}

#[tokio::test]
async fn non_owner_cannot_relate() {
    let lab = lab().await;
    let err = relate(&lab, &lab.bob).await.expect_err("bob is denied");
    assert!(matches!(err, Error::Privacy(_)), "got {err:?}");
    assert_eq!(labels_of(&lab).await, 0);
}

#[tokio::test]
async fn non_owner_cannot_unrelate() {
    let lab = lab().await;
    relate(&lab, &lab.alice).await.expect("owner relates");
    let err = unrelate(&lab, &lab.bob).await.expect_err("bob is denied");
    assert!(matches!(err, Error::Privacy(_)), "got {err:?}");
    assert_eq!(labels_of(&lab).await, 1);
}

#[tokio::test]
async fn forged_note_with_the_attackers_owner_field_is_denied() {
    let lab = lab().await;
    let forged: Note = serde_json::from_value(serde_json::json!({
        "id": lab.note.id().expect("note id"),
        "user": "bob",
        "title": "Groceries",
    }))
    .expect("deserialize forged note");
    let err = forged
        .relate_to_label(
            &lab.label,
            &lab.bob,
            valence::use_!(r"In the **edge-write privacy** suite, we **attach the label through a forged copy of alice's note** so the suite can check the stored owner decides, not the copy. Developers running that example suite use this edge."),
        )
        .await
        .expect_err("bob is denied");
    assert!(matches!(err, Error::Privacy(_)), "got {err:?}");
    assert_eq!(labels_of(&lab).await, 0);

    relate(&lab, &lab.alice).await.expect("owner relates");
    let err = forged
        .unrelate_from_label(
            &lab.label,
            &lab.bob,
            valence::use_!(r"In the **edge-write privacy** suite, we **detach the label through a forged copy of alice's note** so the suite can check the stored owner decides, not the copy. Developers running that example suite use this edge."),
        )
        .await
        .expect_err("bob is denied");
    assert!(matches!(err, Error::Privacy(_)), "got {err:?}");
    assert_eq!(labels_of(&lab).await, 1);
}
