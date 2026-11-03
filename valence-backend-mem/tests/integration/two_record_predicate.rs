//! A query that filters two record fields at once must return only rows matching both.
//!
//! Regression for the shared document-row filter flattening `(team match) AND (user match)`
//! into one `OR`, which made a negative lookup on a known team return an unrelated edge.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::query::{QueryCore, RecordPredicate};
use valence_core::{DatabaseBackend, RecordId, Valence};

const TABLE: &str = "two_record_edge";

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Looks up one team and user edge in an in-memory fixture so the suite can check that a query filtering both record fields returns only rows matching both. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

async fn seeded() -> Valence {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    backend.ensure_schemaless_table(TABLE).await.unwrap();
    for (id, team, user) in [
        ("e1", "team1", "userA"),
        ("e2", "team1", "userB"),
        ("e3", "team2", "userA"),
    ] {
        backend
            .create_record(
                TABLE,
                serde_json::json!({
                    "id": id,
                    "team_id": {"table": "team", "id": team},
                    "user_id": {"table": "user", "id": user},
                }),
            )
            .await
            .unwrap();
    }
    Valence::builder()
        .add_backend("default", backend)
        .with_actor(Actor::User {
            user_id: "alice".into(),
        })
        .build()
        .expect("build")
}

async fn lookup(v: &Valence, team: Option<&str>, user: Option<&str>) -> Vec<String> {
    let mut q = QueryCore::new(TABLE.to_string());
    if let Some(t) = team {
        q = q.where_record(
            "team_id".into(),
            RecordPredicate::Equals(RecordId::new("team", t)),
        );
    }
    if let Some(u) = user {
        q = q.where_record(
            "user_id".into(),
            RecordPredicate::Equals(RecordId::new("user", u)),
        );
    }
    let rows: Vec<serde_json::Value> = q.execute(v, purpose()).await.unwrap();
    let mut ids: Vec<String> = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn team_and_user_returns_exactly_the_matching_edge() {
    let v = seeded().await;
    assert_eq!(lookup(&v, Some("team1"), Some("userA")).await, ["e1"]);
}

#[tokio::test]
async fn user_not_on_the_team_returns_nothing() {
    let v = seeded().await;
    assert_eq!(
        lookup(&v, Some("team2"), Some("userB")).await,
        Vec::<String>::new()
    );
    assert_eq!(
        lookup(&v, Some("team1"), Some("userC")).await,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn team_only_returns_every_edge_of_the_team() {
    let v = seeded().await;
    assert_eq!(lookup(&v, Some("team1"), None).await, ["e1", "e2"]);
}
