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
    use valence::{InMemoryBackend, Model, RecordId, Valence};

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
        let created = Project::create(project, &valence)
            .await
            .expect("create project");
        let project_id = created.id().expect("id").id();

        // Step 3 — Create child Task with BelongsTo RecordId pointing at the project row.
        let task =
            Task::new("ship".to_string(), RecordId::new("project", project_id)).expect("new");
        Task::create(task, &valence).await.expect("create task");

        // Step 4 — Read and merge on the parent model.
        let fetched = Project::get(project_id, &valence).await.expect("get");
        assert_eq!(fetched.as_ref().map(|p| p.name().as_str()), Some("alpha"));

        let merged = Project::merge(project_id, serde_json::json!({ "name": "beta" }), &valence)
            .await
            .expect("merge");
        assert_eq!(merged.name(), "beta");

        // Step 5 — Delete enqueues a DeletionRequest (on_delete: Cascade in schema); capture via dispatcher hook.
        let captured = capture_dispatcher();
        Project::delete(project_id, &valence)
            .await
            .expect("delete queue");
        let (len, root_table) = {
            let reqs = captured.lock().unwrap();
            (reqs.len(), reqs[0].root_table.clone())
        };
        assert_eq!(len, 1);
        assert_eq!(root_table, "project");
    }
}
