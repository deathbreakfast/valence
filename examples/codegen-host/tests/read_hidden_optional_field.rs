//! An optional field with its own read policy (`Account.password_hash`, owner-only).
//!
//! A viewer the field's policy denies loads the account with `None` there. A bare
//! `Model::update` from that model must keep the stored value instead of nulling it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use codegen_host::Account;
use valence::{Actor, DatabaseBackend, InMemoryBackend, Model, QueryCore, Valence};

struct Lab {
    backend: Arc<InMemoryBackend>,
    bob: Valence,
    id: String,
}

async fn lab() -> Lab {
    let backend = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", Arc::clone(&backend) as Arc<dyn DatabaseBackend>)
        .with_actor(Actor::System {
            operation: "read_hidden_optional_field_lab".into(),
        })
        .build()
        .expect("build");
    let alice = valence.with_actor(Actor::User {
        user_id: "alice".into(),
    });
    let bob = valence.with_actor(Actor::User {
        user_id: "bob".into(),
    });
    let created = Account::create(
        Account::new("alice".into(), "Ali".into(), Some("hash-a".into())).expect("new"),
        &alice,
        valence::use_!(r"When the **read-hidden optional field** suite starts, we **create alice's account** so later steps can update it as bob. Developers running that example suite use this row."),
    )
    .await
    .expect("alice creates her account");
    let id = created.id().expect("id").id().to_string();
    Lab { backend, bob, id }
}

async fn get_as_bob(lab: &Lab) -> Account {
    Account::get(
        &lab.id,
        &lab.bob,
        valence::use_!(r"In the **read-hidden optional field** suite, we **load alice's account as bob** so the suite can check which fields bob sees. Developers running that example suite use this result."),
    )
    .await
    .expect("get")
    .expect("bob may read the account")
}

async fn stored_password_hash(lab: &Lab) -> serde_json::Value {
    lab.backend
        .get_record("account", &lab.id)
        .await
        .expect("raw read")
        .expect("row exists")["password_hash"]
        .clone()
}

async fn update_as_bob(lab: &Lab, account: Account) -> Account {
    Account::update(
        &lab.id,
        account,
        &lab.bob,
        valence::use_!(r"In the **read-hidden optional field** suite, we **save alice's account as bob** so the suite can check a full-row update keeps the field bob can't read. Developers running that example suite use this row."),
    )
    .await
    .expect("bob may update")
}

#[tokio::test]
async fn bare_update_keeps_an_unread_optional_field() {
    let lab = lab().await;
    let seen = get_as_bob(&lab).await;
    assert_eq!(seen.password_hash(), None);

    let edited = Account::new(seen.user().clone(), "Alice R.".into(), None).expect("new");
    let updated = update_as_bob(&lab, edited).await;
    assert_eq!(updated.display_name(), "Alice R.");
    assert_eq!(stored_password_hash(&lab).await, "hash-a");
}

#[tokio::test]
async fn bare_update_still_writes_a_hidden_field_it_sets() {
    let lab = lab().await;
    let seen = get_as_bob(&lab).await;
    let edited =
        Account::new(seen.user().clone(), "Ali".into(), Some("hash-b".into())).expect("new");
    update_as_bob(&lab, edited).await;
    assert_eq!(stored_password_hash(&lab).await, "hash-b");
}

#[tokio::test]
async fn record_json_leaves_out_the_field_the_caller_cannot_read() {
    let lab = lab().await;
    let row = QueryCore::get_record_json(
        "account",
        &lab.id,
        &lab.bob,
        valence::use_!(r"In the **read-hidden optional field** suite, we **load alice's account as JSON as bob** so the suite can check the JSON read hides the field bob can't read. Developers running that example suite use this result."),
    )
    .await
    .expect("read")
    .expect("bob may read the account");
    assert_eq!(row["display_name"], "Ali");
    assert!(row.get("password_hash").is_none(), "{row}");
}
