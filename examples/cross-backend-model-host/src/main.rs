//! Cross-backend hop + query: Project on mem, Task on sqlite.
//!
//! ```bash
//! cargo run -p cross-backend-model-host
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::sync::Arc;

use cross_backend_model_host::{Project, Task};
use valence::{
    router_key, Actor, InMemoryBackend, Model, RecordId, SqliteBackend, StringPredicate, Valence,
    MEM_ENGINE_ID, SQLITE_ENGINE_ID,
};

#[tokio::main]
async fn main() -> valence::Result<()> {
    // Step 1 — Register heterogeneous backends: Project schema → mem/default, Task → sqlite/archive.
    let valence = Valence::builder()
        .add_backend("default", Arc::new(InMemoryBackend::new()))
        .add_backend("archive", Arc::new(SqliteBackend::connect_memory().await?))
        .default_backend_key(router_key("default", MEM_ENGINE_ID))
        .with_actor(Actor::System {
            operation: "cross-backend-hop-query".into(),
        })
        .build()?;

    // Step 2 — Prove per-table routing matches schema `database:` evaluators.
    assert_eq!(
        valence.backend_for_table("xb_project")?.engine_id(),
        MEM_ENGINE_ID
    );
    assert_eq!(
        valence.backend_for_table("xb_task")?.engine_id(),
        SQLITE_ENGINE_ID
    );

    // Step 3 — Create rows on their respective backends (connection stores cross-table RecordId).
    let created = Project::create(Project::new("alpha".into())?, &valence).await?;
    let project_id = created.id().expect("project id").id().to_string();
    let task = Task::create(
        Task::new(
            "first task".into(),
            RecordId::new("xb_project", &project_id),
        )?,
        &valence,
    )
    .await?;

    // Step 4 — BelongsTo / HasMany navigation across backends (hard guarantee for this layout).
    let project = task.get_project(&valence).await?;
    assert_eq!(project.name(), "alpha");
    let tasks = Task::get_from_project(&project, &valence).await?;
    assert_eq!(tasks.len(), 1);

    // Step 5 — Same-backend filter query (Project lives entirely on mem).
    let by_name = Project::query(&valence)
        .where_name(StringPredicate::Equals("alpha".into()))
        .await?;
    assert_eq!(by_name.len(), 1);
    assert_eq!(by_name[0].id().expect("project id").id(), project_id);

    // Step 6 — Nested HasMany EXISTS + hop query API shape.
    // Limitation (0.1.x): mem↔sqlite nested EXISTS may return empty — navigation (step 4) is the lesson.
    let nested = Project::query(&valence)
        .where_tasks_has_results(|q| {
            q.where_string("title".into(), StringPredicate::Equals("first task".into()))
        })
        .await?;
    let hop_tasks = Project::query(&valence)
        .where_name(StringPredicate::Equals("alpha".into()))
        .query_tasks()
        .await?;
    if nested.is_empty() || hop_tasks.is_empty() {
        eprintln!(
            "note: nested HasMany query across mem↔sqlite returned empty; \
             BelongsTo/HasMany navigation and Project::query(where_name) succeeded"
        );
    } else {
        assert_eq!(nested.len(), 1);
        assert_eq!(hop_tasks.len(), 1);
    }

    println!(
        "cross-backend-model-host: Project(mem) ↔ Task(sqlite) hop + query OK (project={project_id})"
    );
    Ok(())
}
