//! A required field with its own read policy (`Profile.legal_name`, owner-only).
//!
//! The row must load for everyone the schema lets read it. A viewer the field's
//! policy denies gets the other fields and `None` for this one, and writes made
//! from such a model must not erase the stored value.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use codegen_host::Profile;
use valence::{Actor, DatabaseBackend, InMemoryBackend, Model, Valence};

struct Lab {
    backend: Arc<InMemoryBackend>,
    alice: Valence,
    bob: Valence,
    id: String,
}

/// Alice creates her own profile; bob is another signed-in user.
async fn lab() -> Lab {
    let backend = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", Arc::clone(&backend) as Arc<dyn DatabaseBackend>)
        .with_actor(Actor::System {
            operation: "read_hidable_field_lab".into(),
        })
        .build()
        .expect("build");
    let alice = valence.with_actor(Actor::User {
        user_id: "alice".into(),
    });
    let bob = valence.with_actor(Actor::User {
        user_id: "bob".into(),
    });
    let created = Profile::create(
        Profile::new("alice".into(), "Alice Rivera".into(), "Ali".into()).expect("new"),
        &alice,
        valence::use_!(r"When the **read-hidable field** suite starts, we **create alice's profile** so later steps can read it as alice and as bob. Developers running that example suite use this row."),
    )
    .await
    .expect("alice creates her profile");
    let id = created.id().expect("id").id().to_string();
    Lab {
        backend,
        alice,
        bob,
        id,
    }
}

async fn get_as(viewer: &Valence, id: &str) -> Option<Profile> {
    Profile::get(
        id,
        viewer,
        valence::use_!(r"In the **read-hidable field** suite, we **load the profile by id** so each step can check which fields this viewer sees. Developers running that example suite use this result."),
    )
    .await
    .expect("get")
}

async fn stored_legal_name(lab: &Lab) -> serde_json::Value {
    lab.backend
        .get_record("profile", &lab.id)
        .await
        .expect("raw read")
        .expect("row exists")["legal_name"]
        .clone()
}

#[tokio::test]
async fn owner_reads_every_field() {
    let lab = lab().await;
    let profile = get_as(&lab.alice, &lab.id)
        .await
        .expect("alice sees her row");
    assert_eq!(
        profile.legal_name().map(String::as_str),
        Some("Alice Rivera")
    );
    assert_eq!(profile.display_name(), "Ali");
    assert_eq!(profile.user(), "alice");
}

#[tokio::test]
async fn non_owner_gets_the_row_without_the_hidden_field() {
    let lab = lab().await;
    let profile = get_as(&lab.bob, &lab.id)
        .await
        .expect("bob may read the row; only legal_name is hidden");
    assert_eq!(profile.legal_name(), None);
    assert_eq!(profile.display_name(), "Ali");
    assert_eq!(profile.user(), "alice");
}

#[tokio::test]
async fn non_owner_full_update_keeps_the_hidden_value() {
    let lab = lab().await;
    let seen_by_bob = get_as(&lab.bob, &lab.id).await.expect("row");
    assert_eq!(seen_by_bob.legal_name(), None);

    Profile::update(
        &lab.id,
        seen_by_bob,
        &lab.bob,
        valence::use_!(r"In the **read-hidable field** suite, we **save the profile as bob** so the suite can check a full-row update keeps the field bob can't read. Developers running that example suite use this row."),
    )
    .await
    .expect("bob may update");

    assert_eq!(stored_legal_name(&lab).await, "Alice Rivera");
    let profile = get_as(&lab.alice, &lab.id).await.expect("row");
    assert_eq!(
        profile.legal_name().map(String::as_str),
        Some("Alice Rivera")
    );
}

#[tokio::test]
async fn non_owner_sparse_update_keeps_the_hidden_value() {
    let lab = lab().await;
    let updated = get_as(&lab.bob, &lab.id)
        .await
        .expect("row")
        .get_mutable(
            &lab.bob,
            valence::use_!(r"In the **read-hidable field** suite, we **open the profile for editing as bob** so the suite can check a partial update keeps the field bob can't read. Developers running that example suite use this result."),
        )
        .set_display_name("Alice R.".into())
        .expect("set_display_name")
        .commit()
        .await
        .expect("commit");
    assert_eq!(updated.display_name(), "Alice R.");
    assert_eq!(stored_legal_name(&lab).await, "Alice Rivera");
}

#[tokio::test]
async fn create_without_the_hidden_field_is_rejected() {
    let lab = lab().await;
    // A model as a non-owner would hold it: legal_name absent.
    let partial: Profile = serde_json::from_value(serde_json::json!({
        "user": "carol",
        "display_name": "Caz",
    }))
    .expect("decodes without legal_name");

    let err = Profile::create(
        partial,
        &lab.bob,
        valence::use_!(r"In the **read-hidable field** suite, we **try to create a profile with no legal name** so the suite can check the write is refused. Developers running that example suite use this result."),
    )
    .await
    .expect_err("legal_name is required on create");
    assert!(
        matches!(&err, valence::Error::Validation(m) if m.contains("profile.legal_name")),
        "unexpected error: {err}"
    );

    let rows = query_as(&lab.bob).await;
    assert_eq!(rows.len(), 1, "only alice's row exists");
}

#[tokio::test]
async fn non_owner_query_decodes_rows_without_the_hidden_field() {
    let lab = lab().await;
    let rows = query_as(&lab.bob).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].legal_name(), None);
    assert_eq!(rows[0].display_name(), "Ali");

    let rows = query_as(&lab.alice).await;
    assert_eq!(
        rows[0].legal_name().map(String::as_str),
        Some("Alice Rivera")
    );
}

async fn query_as(viewer: &Valence) -> Vec<Profile> {
    Profile::query(
        viewer,
        valence::use_!(r"In the **read-hidable field** suite, we **list every profile** so the suite can check which fields each viewer sees and how many rows exist. Developers running that example suite use this result."),
    )
    .await
    .expect("query")
}
