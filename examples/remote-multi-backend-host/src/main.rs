//! Live multi-remote routing: `Project` on Postgres (`primary`), `Task` on Redis (`cache`) — two
//! real network-backed engines behind one router, both wired through `.add_backend(...)`.
//!
//! ```bash
//! docker run --rm -d --name valence-postgres -p 5432:5432 \
//!   -e POSTGRES_PASSWORD=postgres postgres:16
//! docker run --rm -d --name valence-redis -p 6379:6379 redis:7
//!
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres \
//! VALENCE_REDIS_URL=redis://127.0.0.1:6379 \
//!   cargo run -p remote-multi-backend-host
//! ```
//!
//! Skips cleanly when either `DATABASE_URL` or `VALENCE_REDIS_URL` is unset.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::prelude::*;
use valence::{
    router_key, DatabaseBackend, PostgresBackend, RedisBackend, Valence, POSTGRES_ENGINE_ID,
    REDIS_ENGINE_ID,
};

const PROJECT_DB: DatabaseFromEngine = Database::from_engine("primary", POSTGRES_ENGINE_ID);
const TASK_DB: DatabaseFromEngine = Database::from_engine("cache", REDIS_ENGINE_ID);

// Step 1 — Two schemas, two physical engines, selected purely by their `database:` evaluator.
// Neither schema knows the other engine exists — routing is entirely router-key driven.
valence_schema! {
    RmbProject {
        table: "rmb_project",
        version: "0.1.0",
        description: "Project routed to the Postgres primary",
        database: PROJECT_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
    }
}

valence_schema! {
    RmbTask {
        table: "rmb_task",
        version: "0.1.0",
        description: "Task routed to the Redis cache",
        database: TASK_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
        ],
    }
}

async fn get_or_create(
    backend: &dyn DatabaseBackend,
    table: &str,
    id: &str,
    content: serde_json::Value,
) -> valence::Result<serde_json::Value> {
    if let Some(existing) = backend.get_record(table, id).await? {
        return Ok(existing);
    }
    backend.create_record(table, content).await
}

#[tokio::main]
async fn main() -> valence::Result<()> {
    let (Ok(_), Ok(_)) = (
        std::env::var("DATABASE_URL"),
        std::env::var("VALENCE_REDIS_URL"),
    ) else {
        eprintln!(
            "skip: set DATABASE_URL and VALENCE_REDIS_URL to run this example (see module docs \
             for a Docker one-liner)"
        );
        return Ok(());
    };

    // Step 2 — Connect both live services and register them under distinct router keys.
    let postgres = PostgresBackend::from_env().await?;
    let redis = RedisBackend::from_env().await?;

    let valence = Valence::builder()
        .add_backend("primary", Arc::new(postgres))
        .add_backend("cache", Arc::new(redis))
        .default_backend_key(router_key("primary", POSTGRES_ENGINE_ID))
        .build()?;

    // Step 3 — Prove per-table routing matches each schema's `database:` evaluator.
    assert_eq!(
        valence.backend_for_table("rmb_project")?.engine_id(),
        POSTGRES_ENGINE_ID
    );
    assert_eq!(
        valence.backend_for_table("rmb_task")?.engine_id(),
        REDIS_ENGINE_ID
    );

    // Step 4 — Live round trip on each engine, through the same router-resolved backend handle.
    // `get_or_create` keeps re-runs against the same live database idempotent (no duplicate-key
    // error on a second `cargo run` against data left over from the first).
    let project_backend = valence.backend_for_table("rmb_project")?;
    let project = get_or_create(
        project_backend.as_ref(),
        "rmb_project",
        "p1",
        serde_json::json!({"id": "p1", "name": "alpha"}),
    )
    .await?;

    let task_backend = valence.backend_for_table("rmb_task")?;
    let task = get_or_create(
        task_backend.as_ref(),
        "rmb_task",
        "t1",
        serde_json::json!({"id": "t1", "title": "first task"}),
    )
    .await?;

    println!(
        "remote-multi-backend-host: project={} (postgres) task={} (redis)",
        project["name"].as_str().unwrap_or_default(),
        task["title"].as_str().unwrap_or_default()
    );
    println!("remote-multi-backend-host: OK (live Postgres + Redis routing proven)");
    Ok(())
}
