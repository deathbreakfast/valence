//! JSON report shape for benchmark runs.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use valence_testkit::MatrixSpec;

use crate::resource::ResourceMetrics;
use crate::stats::MetricStats;
use crate::sweep::SweepParams;

/// Snapshot of sweep dimensions used for a run.
#[derive(Debug, Clone, Serialize)]
pub struct SweepSnapshot {
    pub prefill: usize,
    pub duration_secs: u64,
    pub concurrency: usize,
    pub bench_clients: usize,
    pub query_iters: usize,
    pub privacy_sleep_us: u64,
    pub bench_client_index: usize,
}

impl From<&SweepParams> for SweepSnapshot {
    fn from(s: &SweepParams) -> Self {
        Self {
            prefill: s.prefill,
            duration_secs: s.duration_secs,
            concurrency: s.concurrency,
            bench_clients: s.bench_clients,
            query_iters: s.query_iters,
            privacy_sleep_us: s.privacy_sleep_us,
            bench_client_index: SweepParams::client_index(),
        }
    }
}

/// Write-track metrics.
#[derive(Debug, Clone, Serialize)]
pub struct WriteMetrics {
    pub achieved_write_ops_per_sec: f64,
    pub error_rate: f64,
    pub total_ops: u64,
    pub error_count: usize,
}

/// JSON report emitted after each benchmark run.
#[derive(Debug, Serialize)]
pub struct BenchReport {
    pub experiment: String,
    pub matrix_slug: String,
    pub hardware: String,
    pub storage: String,
    pub telemetry: String,
    pub topology: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bench_topology: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_ms: Option<MetricStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_ms: Option<MetricStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops_per_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sweep: Option<SweepSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write: Option<WriteMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefill_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bench_clients: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceMetrics>,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub recorded_at: String,
}

impl BenchReport {
    pub fn hardware_profile() -> String {
        std::env::var("VALENCE_BENCH_HARDWARE").unwrap_or_else(|_| "dev-wsl".into())
    }

    pub fn bench_topology_for(matrix: &MatrixSpec) -> String {
        match matrix.storage {
            valence_testkit::StorageAdapter::Postgres
            | valence_testkit::StorageAdapter::MongoDb
            | valence_testkit::StorageAdapter::Redis => "remote".into(),
            _ => "embedded".into(),
        }
    }

    pub fn base(experiment: &str, matrix: &MatrixSpec) -> Self {
        Self {
            experiment: experiment.to_string(),
            matrix_slug: matrix.slug(),
            hardware: Self::hardware_profile(),
            storage: matrix.storage.slug().to_string(),
            telemetry: matrix.telemetry.slug().to_string(),
            topology: matrix.topology.slug().to_string(),
            bench_topology: Some(Self::bench_topology_for(matrix)),
            scenario_id: None,
            ops: None,
            op_ms: None,
            query_ms: None,
            ops_per_sec: None,
            sweep: None,
            write: None,
            prefill_count: None,
            error_rate: None,
            bench_clients: None,
            resource: None,
            status: "ok",
            pass_notes: None,
            error: None,
            recorded_at: format!(
                "{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs())
            ),
        }
    }

    pub fn with_sweep(mut self, sweep: &SweepParams) -> Self {
        self.sweep = Some(SweepSnapshot::from(sweep));
        self.bench_clients = Some(sweep.bench_clients);
        self
    }

    pub fn default_report_path(experiment: &str, matrix: &MatrixSpec) -> PathBuf {
        PathBuf::from(format!(
            "profiling/valence-bench/reports/{}-{}-{}.json",
            experiment,
            matrix.slug(),
            Self::hardware_profile()
        ))
    }
}
