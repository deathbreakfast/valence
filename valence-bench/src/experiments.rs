//! Experiment registry and default operation counts.

use anyhow::{bail, Result};
use valence_testkit::StorageAdapter;

/// All registered benchmark experiment ids.
pub const ALL_EXPERIMENT_IDS: &[&str] = &[
    "bm-v0", "bm-v1", "bm-v2", "bm-v3", "bm-v4", "bm-v5", "bm-v6", "bm-v7", "bm-v8", "bm-v9",
    "bm-v11", "bm-v12", "bm-v13", "bm-v14", "bm-v15", "bm-v16", "bm-v17", "bm-v18", "bm-v19",
    "bm-v20", "bm-v21", "bm-v22", "bm-v23", "bm-v24", "bm-v25", "bm-v26", "bm-v27", "bm-v28",
];

/// Experiments that always use in-process mem (ignore matrix `--storage`).
pub fn is_mem_only_experiment(id: &str) -> bool {
    matches!(id, "bm-v1" | "bm-v2" | "bm-v18")
}

/// Whether `storage` is a valid matrix cell for `experiment`.
pub fn experiment_storage_ok(experiment: &str, storage: StorageAdapter) -> bool {
    if is_mem_only_experiment(experiment) {
        return matches!(storage, StorageAdapter::Mem);
    }
    if experiment == "bm-v26" {
        return matches!(
            storage,
            StorageAdapter::HybridIndraPg | StorageAdapter::Postgres | StorageAdapter::IndraDb
        );
    }
    true
}

/// Wall-clock timeout for query-real matrix cells (full-scan adapters can hang in debug).
pub fn query_real_timeout_secs(experiment: &str) -> Option<u64> {
    match experiment {
        "bm-v21" | "bm-v22" | "bm-v23" => Some(120),
        _ => None,
    }
}

/// Resolved plan for one benchmark run.
pub struct ExperimentPlan {
    /// Experiment slug (for example `"bm-v0"`).
    pub id: String,
    /// Default measured operation count when `--ops` is omitted.
    pub default_ops: usize,
}

/// Resolve an experiment id and optional CLI overrides into a runnable plan.
pub fn resolve_experiment(id: &str, ops: Option<usize>) -> Result<ExperimentPlan> {
    let default_ops = match id {
        "bm-v0" | "bm-v1" | "bm-v2" | "bm-v4" | "bm-v6" | "bm-v8" | "bm-v9" | "bm-v16"
        | "bm-v18" | "bm-v20" | "bm-v21" => 1000,
        "bm-v3" | "bm-v13" | "bm-v14" | "bm-v15" | "bm-v19" | "bm-v23" => 500,
        "bm-v5" | "bm-v7" | "bm-v28" => 0,
        "bm-v11" | "bm-v12" | "bm-v22" => 1000,
        "bm-v17" | "bm-v24" | "bm-v25" | "bm-v26" | "bm-v27" => 200,
        other => bail!("unknown experiment id: {other}"),
    };
    Ok(ExperimentPlan {
        id: id.into(),
        default_ops: ops.unwrap_or(default_ops),
    })
}
