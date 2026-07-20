//! Cross-backend model host: project and task on different storage engines.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence::{Database, DatabaseFromEngine, KnownEngines};

/// Project table routes to primary mem backend.
pub const PROJECT_DB: DatabaseFromEngine =
    Database::from_engine("default", KnownEngines::INMEMORY_MEM);

/// Task table routes to archive sqlite backend.
pub const TASK_DB: DatabaseFromEngine = Database::from_engine("archive", KnownEngines::SQLITE);

valence::include_generated_models!();

pub use generated::{XbProject as Project, XbTask as Task};
pub use generated::{XbProjectSchema as ProjectSchema, XbTaskSchema as TaskSchema};
