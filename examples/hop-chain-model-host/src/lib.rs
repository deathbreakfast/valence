//! Hop-chain host: Org→Project→Task→Note on hop_a..hop_d.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence::{Database, DatabaseFromEngine};

pub const HOP_A: &str = "hop_a";
pub const HOP_B: &str = "hop_b";
pub const HOP_C: &str = "hop_c";
pub const HOP_D: &str = "hop_d";

pub const ORG_DB: DatabaseFromEngine = Database::from_engine("n1", HOP_A);
pub const PROJECT_DB: DatabaseFromEngine = Database::from_engine("n2", HOP_B);
pub const TASK_DB: DatabaseFromEngine = Database::from_engine("n3", HOP_C);
pub const NOTE_DB: DatabaseFromEngine = Database::from_engine("n4", HOP_D);

valence::include_generated_models!();

pub use generated::{
    HopChainNote as Note, HopChainOrg as Org, HopChainProject as Project, HopChainTask as Task,
};
