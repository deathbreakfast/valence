//! Generated models must register schema `database:` evaluators (not DEFAULT_IN_MEMORY).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use cross_backend_model_host::{ProjectSchema, TaskSchema, PROJECT_DB, TASK_DB};
use valence::DatabaseEvaluator;

#[test]
fn project_and_task_schemas_use_declared_evaluators() {
    let project = ProjectSchema::full();
    let task = TaskSchema::full();

    assert_eq!(
        project.database_evaluator.name(),
        PROJECT_DB.name(),
        "project must route via PROJECT_DB"
    );
    assert_eq!(
        task.database_evaluator.name(),
        TASK_DB.name(),
        "task must route via TASK_DB"
    );
    assert_ne!(
        project.database_evaluator.name(),
        task.database_evaluator.name(),
        "cross-backend host tables must not share one evaluator"
    );
}
