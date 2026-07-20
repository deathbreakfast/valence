//! Model contract: create → get → merge → delete queue on mem backend.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use product_model_host::{Project, Task};
// Force-link schema inventory from build.rs codegen.
use product_model_host as _;
use valence::actor::Actor;
use valence::deletion::{register_deletion_dispatcher, DeletionRequest};
use valence::{InMemoryBackend, Model, RecordId, Valence};

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
    let valence = Valence::builder()
        .add_backend("default", Arc::new(InMemoryBackend::new()))
        .with_actor(Actor::System {
            operation: "model_contract".into(),
        })
        .build()
        .expect("build");

    let project = Project::new("alpha".to_string()).expect("new");
    let created = Project::create(project, &valence)
        .await
        .expect("create project");
    let project_id = created.id().expect("id").id();

    let task = Task::new("ship".to_string(), RecordId::new("project", project_id)).expect("new");
    Task::create(task, &valence).await.expect("create task");

    let fetched = Project::get(project_id, &valence).await.expect("get");
    assert_eq!(fetched.as_ref().map(|p| p.name().as_str()), Some("alpha"));

    let merged = Project::merge(project_id, serde_json::json!({ "name": "beta" }), &valence)
        .await
        .expect("merge");
    assert_eq!(merged.name(), "beta");

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
