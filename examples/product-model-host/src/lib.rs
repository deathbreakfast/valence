//! Product-shaped codegen host: connections + ownership hooks in generated CRUD.
//!
//! End-to-end proof: `cargo test -p product-model-host -- --test-threads=1`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use serde::{Deserialize, Serialize};

/// JSON payload for [`TypedProbe`] `FieldType::JsonAs` coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbePayload {
    /// Arbitrary integer for round-trip asserts.
    pub n: i64,
    /// Arbitrary label for round-trip asserts.
    pub label: String,
}

valence::include_generated_models!();

#[cfg(test)]
mod tests {
    //! Model contract: create → get → merge → delete queue on mem backend.

    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use valence::actor::Actor;
    use valence::deletion::{register_deletion_dispatcher, DeletionRequest};
    use valence::{use_, InMemoryBackend, Model, RecordId, Valence};

    use super::{Project, Task};

    fn capture_dispatcher() -> Arc<Mutex<Vec<DeletionRequest>>> {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let hook_target = Arc::clone(&captured);
        let dispatcher: Box<
            dyn Fn(DeletionRequest) -> Pin<Box<dyn Future<Output = valence::Result<()>> + Send>>
                + Send
                + Sync,
        > = Box::new(move |req| {
            let hook_target = Arc::clone(&hook_target);
            Box::pin(async move {
                hook_target.lock().unwrap().push(req);
                Ok(())
            })
        });
        register_deletion_dispatcher(dispatcher);
        captured
    }

    #[tokio::test]
    async fn product_model_crud_and_delete_queue() {
        // Step 1 — Boot mem Valence; connections and delete hooks run in-process.
        let valence = Valence::builder()
            .add_backend("default", Arc::new(InMemoryBackend::new()))
            .with_actor(Actor::System {
                operation: "model_contract".into(),
            })
            .build()
            .expect("build");

        // Step 2 — Create parent Project (HasMany side of connection in project_valence_schema.rs).
        let project = Project::new("alpha".to_string()).expect("new");
        let created = Project::create(
            project,
            &valence,
            valence::use_!(r"When the **product-model-host** suite builds a sample workspace, we **create the parent project** so a linked task can exercise cascade delete. Developers running that example suite use this row."),
        )
        .await
        .expect("create project");
        let project_id = created.id().expect("id").id();

        // Step 3 — Create child Task with BelongsTo RecordId pointing at the project row.
        let task =
            Task::new("ship".to_string(), RecordId::new("project", project_id)).expect("new");
        Task::create(
            task,
            &valence,
            valence::use_!(r"After the sample **project** exists, we **create a linked task** so cascade delete coverage has a child connection to remove with the parent. Developers running that example suite use this row."),
        )
        .await
        .expect("create task");

        // Step 4 — Read and merge on the parent model.
        let fetched = Project::get(
            project_id,
            &valence,
            valence::use_!(r"After create, we **reload the project by id** so the product-model-host suite can confirm the stored name matches what we wrote. Developers running that example suite use this result."),
        )
        .await
        .expect("get");
        assert_eq!(fetched.as_ref().map(|p| p.name().as_str()), Some("alpha"));
        let fetched = fetched.expect("row exists");

        let merged = fetched
            .get_mutable(
                &valence,
                valence::use_!(r"During the **product-model-host** suite, we **load the project as a mutable builder** so partial update coverage can assert the renamed row. Developers running that example suite use this result."),
            )
            .set_name("beta".to_string())
            .expect("set_name")
            .commit()
            .await
            .expect("commit");
        assert_eq!(merged.name(), "beta");

        // Step 5 — Delete enqueues a DeletionRequest (on_delete: Cascade in schema); capture via dispatcher hook.
        let captured = capture_dispatcher();
        Project::delete(
            project_id,
            &valence,
            valence::use_!(r"At the end of the **product-model-host** suite, we **queue project deletion** so the cascade dispatcher hook can capture the deletion request. Developers running that example suite use this result."),
        )
        .await
        .expect("delete queue");
        let (len, root_table) = {
            let reqs = captured.lock().unwrap();
            (reqs.len(), reqs[0].root_table.clone())
        };
        assert_eq!(len, 1);
        assert_eq!(root_table, "project");
    }

    #[tokio::test]
    async fn generated_fetch_with_stats_matches_into_future_happy() {
        use std::sync::Arc;

        use valence::DatabaseBackend;

        use super::OwnedNote;

        // Ten rows; alice owns four of them.
        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        for i in 0..10 {
            let owner = if [1, 3, 4, 8].contains(&i) {
                "alice"
            } else {
                "bob"
            };
            backend
                .create_record(
                    "owned_note",
                    serde_json::json!({"id": {"table": "owned_note", "id": format!("n{i:02}")}, "user": owner}),
                )
                .await
                .unwrap();
        }
        let valence = Valence::builder()
            .add_backend("default", backend)
            .with_actor(Actor::User {
                user_id: "alice".into(),
            })
            .build()
            .expect("build");

        let awaited = OwnedNote::query(&valence, valence::use_!(r"When the **product-model-host** suite lists sample notes, we **query them by owner** so the stats test can compare the awaited rows with the fetch count. Developers running that example suite use this result.")).limit(10).await.expect("await");
        let (rows, stats) = OwnedNote::query(&valence, valence::use_!(r"When the **product-model-host** suite lists sample notes, we **query them by owner** so the stats test can compare the awaited rows with the fetch count. Developers running that example suite use this result."))
            .limit(10)
            .fetch_with_stats()
            .await
            .expect("fetch_with_stats");
        assert_eq!(rows.len(), 4);
        assert_eq!(stats.fetched, 10);
        let ids = |v: &[OwnedNote]| {
            v.iter()
                .map(|n| n.id().expect("id").id().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&rows), ids(&awaited));
    }

    /// Raw-seed one `edge_doc` owned by alice and an `edge_label` each for alice and bob.
    async fn seed_edge_rows() -> std::sync::Arc<dyn valence::DatabaseBackend> {
        use valence::DatabaseBackend;

        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        backend
            .create_record(
                "edge_doc",
                serde_json::json!({"id": {"table": "edge_doc", "id": "d1"}, "user": "alice"}),
            )
            .await
            .unwrap();
        for (id, owner) in [("mine", "alice"), ("bobs", "bob")] {
            backend
                .create_record(
                    "edge_label",
                    serde_json::json!({"id": {"table": "edge_label", "id": id}, "user": owner}),
                )
                .await
                .unwrap();
        }
        backend
    }

    fn edge_valence(backend: &Arc<dyn valence::DatabaseBackend>, user_id: &str) -> Valence {
        Valence::builder()
            .add_backend("default", Arc::clone(backend))
            .with_actor(Actor::User {
                user_id: user_id.into(),
            })
            .build()
            .expect("build")
    }

    #[tokio::test]
    async fn edge_policy_generated_relate_to_checked_happy() {
        use super::{EdgeDoc, EdgeLabel};

        let backend = seed_edge_rows().await;
        let alice = edge_valence(&backend, "alice");
        let purpose = valence::use_!(
            r"When the **product-model-host** suite links sample labels, we **load the doc and label** so the generated relate and unrelate methods can be checked against edge privacy. Developers running that example suite use this result."
        );

        let doc = EdgeDoc::get("d1", &alice, purpose)
            .await
            .unwrap()
            .expect("doc");
        let label = EdgeLabel::get("mine", &alice, purpose)
            .await
            .unwrap()
            .expect("label");
        doc.relate_to_label(&label, &alice, purpose)
            .await
            .expect("owner links a label they can read");
        let linked = doc.get_labels(&alice, purpose).await.unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id().expect("id").id(), "mine");

        doc.unrelate_from_label(&label, &alice, purpose)
            .await
            .expect("owner unlinks");
        assert_eq!(doc.get_labels(&alice, purpose).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn edge_policy_generated_relate_to_checked_sad() {
        use valence::DatabaseBackend;

        use super::{EdgeDoc, EdgeLabel};

        let backend = seed_edge_rows().await;
        let bob = edge_valence(&backend, "bob");
        let purpose = valence::use_!(
            r"When the **product-model-host** suite links sample labels, we **load the doc and label** so the generated relate and unrelate methods can be checked against edge privacy. Developers running that example suite use this result."
        );

        // Bob can read alice's doc and his own label, but can't update the doc.
        let doc = EdgeDoc::get("d1", &bob, purpose)
            .await
            .unwrap()
            .expect("doc");
        let label = EdgeLabel::get("bobs", &bob, purpose)
            .await
            .unwrap()
            .expect("label");
        let err = doc
            .relate_to_label(&label, &bob, purpose)
            .await
            .unwrap_err();
        assert!(matches!(err, valence::Error::Privacy(_)), "{err:?}");
        let targets = backend
            .get_edge_targets(&RecordId::new("edge_doc", "d1"), "edge_doc_label")
            .await
            .unwrap();
        assert_eq!(targets, Vec::<RecordId>::new());
    }

    /// Bob can't read alice's note, so `Model::get` hides it. Update privacy must
    /// still be checked against the stored row, with or without a caller `before`.
    #[tokio::test]
    async fn update_checks_stored_row_when_caller_cannot_read_it() {
        use std::sync::Arc;

        use valence::DatabaseBackend;

        use super::OwnedNote;

        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        backend
            .create_record(
                "owned_note",
                serde_json::json!({"id": {"table": "owned_note", "id": "n1"}, "user": "alice"}),
            )
            .await
            .unwrap();
        let as_user = |user: &str| {
            Valence::builder()
                .add_backend("default", backend.clone())
                .with_actor(Actor::User {
                    user_id: user.into(),
                })
                .build()
                .expect("build")
        };
        let note = |user: &str| -> OwnedNote {
            serde_json::from_value(
                serde_json::json!({"id": {"table": "owned_note", "id": "n1"}, "user": user}),
            )
            .expect("note")
        };
        let purpose = || {
            valence::use_!(
                r"When the **product-model-host** suite updates a note it cannot read, we **check Update privacy against the stored row** so the test can prove a non-owner is refused. Developers running that example suite use this result."
            )
        };
        let bob = as_user("bob");

        // `data` names bob as owner, so only the stored row can refuse this.
        let err = OwnedNote::update("n1", note("bob"), &bob, purpose())
            .await
            .expect_err("non-owner update must be refused");
        assert!(matches!(err, valence::Error::Privacy(_)), "{err:?}");

        // A forged `before` that claims bob owns the row must not be trusted.
        let err = OwnedNote::update_with_before("n1", note("bob"), Some(note("bob")), &bob)
            .await
            .expect_err("forged before must be refused");
        assert!(matches!(err, valence::Error::Privacy(_)), "{err:?}");

        let stored = backend
            .get_record("owned_note", "n1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored["user"], "alice");

        // The owner can still update their own row.
        OwnedNote::update("n1", note("alice"), &as_user("alice"), purpose())
            .await
            .expect("owner update");
    }

    /// An owner can't read `admin_notes` (system-only), so their update payload lacks
    /// it. Comparing that against the full stored row must not read as clearing the
    /// field and trip its update policy.
    #[tokio::test]
    async fn owner_edits_title_on_row_with_admin_only_field() {
        use std::sync::Arc;

        use valence::DatabaseBackend;

        use super::AdminNote;

        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        backend
            .create_record(
                "admin_note",
                serde_json::json!({
                    "id": {"table": "admin_note", "id": "a1"},
                    "user": "alice",
                    "title": "old",
                    "admin_notes": "internal",
                }),
            )
            .await
            .unwrap();
        let alice = Valence::builder()
            .add_backend("default", backend.clone())
            .with_actor(Actor::User {
                user_id: "alice".into(),
            })
            .build()
            .expect("build");
        let purpose = || {
            valence::use_!(
                r"When the **product-model-host** suite lets an owner edit a note that has an admin-only field, we **load and update that note** so the test can prove the edit succeeds. Developers running that example suite use this result."
            )
        };

        let loaded = AdminNote::get("a1", &alice, purpose())
            .await
            .expect("get")
            .expect("owner reads the row");
        let mut value = serde_json::to_value(&loaded).expect("to_value");
        assert!(value.get("admin_notes").is_none_or(|v| v.is_null()));
        value["title"] = serde_json::json!("new");
        let edited: AdminNote = serde_json::from_value(value).expect("from_value");

        AdminNote::update("a1", edited, &alice, purpose())
            .await
            .expect("owner edit must succeed");

        let stored = backend
            .get_record("admin_note", "a1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored["title"], "new");
        assert_eq!(stored["admin_notes"], "internal");
    }
}
