//! Deterministic mixed OLTP schedule and firehose (`prod-mix-v1`).
//!
//! Cycle of 20 ops: 10% create, 55% hot get, 10% cache-off primary get, 25%
//! equality filter. Isolated bm-v5 / bm-v28 / bm-v20 / bm-v21 cells each own
//! the store; this hose shares one client across all four classes, so mixed
//! ops/s sits below those peaks.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, ensure, Result};
use valence_core::{
    compile_for_engine, CompiledQuery, DatabaseBackend, QueryCore, StringPredicate,
};

use crate::stats::MetricStats;

/// Marketing mixed scenario id recorded on [`crate::report::BenchReport`].
pub const SCENARIO_ID: &str = "prod-mix-v1";

/// Hot working set size (same 64-key shape as bm-v28).
pub const HOT_SET_SIZE: usize = 64;

/// Fail closed when aggregate errors reach this fraction (0.1%).
pub const MAX_ERROR_RATE: f64 = 0.001;

/// Allowed deviation from target mix shares (two percentage points).
pub const MIX_DRIFT_TOLERANCE: f64 = 0.02;

/// Target successful-op shares for `prod-mix-v1`.
pub const TARGET_CREATE: f64 = 0.10;
pub const TARGET_HOT_GET: f64 = 0.55;
pub const TARGET_COLD_GET: f64 = 0.10;
pub const TARGET_EQ_FILTER: f64 = 0.25;

/// One step in the repeating 20-slot mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixOp {
    /// Adapter `create_record`.
    Create,
    /// Point-get against the 64-key hot set (mirrors kept).
    HotGet,
    /// Point-get against unique corpus keys with cache-off / Indra drop.
    ColdGet,
    /// Equality filter over the prefilled corpus.
    EqFilter,
}

/// 2 create + 11 hot + 2 cold + 5 eq = 10 / 55 / 10 / 25.
pub const MIX_CYCLE: [MixOp; 20] = [
    MixOp::Create,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::HotGet,
    MixOp::ColdGet,
    MixOp::EqFilter,
    MixOp::EqFilter,
    MixOp::EqFilter,
    MixOp::Create,
    MixOp::ColdGet,
    MixOp::EqFilter,
    MixOp::EqFilter,
];

/// Class of the `index`-th issued operation (0-based, wrapping the cycle).
#[must_use]
pub fn mix_op_at(index: u64) -> MixOp {
    MIX_CYCLE[(index as usize) % MIX_CYCLE.len()]
}

/// Prefill row id used by [`crate::workload::prefill::prefill_table`].
#[must_use]
pub fn prefill_id(index: usize) -> String {
    format!("prefill-{index}")
}

/// First [`HOT_SET_SIZE`] prefill ids (or fewer when the corpus is smaller).
#[must_use]
pub fn hot_ids(prefill: usize) -> Vec<String> {
    (0..HOT_SET_SIZE.min(prefill)).map(prefill_id).collect()
}

/// Prefill ids after the hot set — cache-off / primary-miss keys.
#[must_use]
pub fn cold_ids(prefill: usize) -> Vec<String> {
    (HOT_SET_SIZE..prefill).map(prefill_id).collect()
}

/// Per-class throughput and latency for one mixed run.
#[derive(Debug, Clone, Copy)]
pub struct MixedClassResult {
    pub ops: u64,
    pub error_count: usize,
    pub share: f64,
    pub op_ms: MetricStats,
}

/// Aggregate mixed-OLTP firehose result.
#[derive(Debug, Clone, Copy)]
pub struct MixedFirehoseResult {
    pub achieved_mixed_ops_per_sec: f64,
    pub total_ops: u64,
    pub error_count: usize,
    pub error_rate: f64,
    pub duration_secs: f64,
    pub create: MixedClassResult,
    pub hot_get: MixedClassResult,
    pub cold_get: MixedClassResult,
    pub eq_filter: MixedClassResult,
    pub dropped_hybrid_mirrors: bool,
}

/// Why a mixed run is not an `ok` cell.
#[derive(Debug, Clone, PartialEq)]
pub enum MixContractError {
    MissingClass {
        class: &'static str,
    },
    MixDrift {
        class: &'static str,
        actual: f64,
        target: f64,
    },
    ErrorRate {
        actual: f64,
    },
}

impl std::fmt::Display for MixContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingClass { class } => {
                write!(f, "bm-v29 missing operation class {class}")
            }
            Self::MixDrift {
                class,
                actual,
                target,
            } => write!(
                f,
                "bm-v29 mix drift for {class}: actual {actual:.4} target {target:.4} (tolerance {MIX_DRIFT_TOLERANCE:.4})"
            ),
            Self::ErrorRate { actual } => write!(
                f,
                "bm-v29 aggregate error rate {actual:.4} >= {MAX_ERROR_RATE}"
            ),
        }
    }
}

impl std::error::Error for MixContractError {}

/// Successful-op mix and aggregate error-rate gates.
pub fn validate_mix(
    create_ok: u64,
    hot_ok: u64,
    cold_ok: u64,
    eq_ok: u64,
    error_count: usize,
) -> Result<(), MixContractError> {
    let total_ok = create_ok + hot_ok + cold_ok + eq_ok;
    let attempts = total_ok.saturating_add(error_count as u64);
    let error_rate = if attempts == 0 {
        0.0
    } else {
        error_count as f64 / attempts as f64
    };
    if error_rate >= MAX_ERROR_RATE {
        return Err(MixContractError::ErrorRate { actual: error_rate });
    }
    for (count, class) in [
        (create_ok, "create"),
        (hot_ok, "hot_get"),
        (cold_ok, "cold_get"),
        (eq_ok, "eq_filter"),
    ] {
        if count == 0 {
            return Err(MixContractError::MissingClass { class });
        }
    }
    if total_ok == 0 {
        return Err(MixContractError::MissingClass { class: "all" });
    }
    for (count, class, target) in [
        (create_ok, "create", TARGET_CREATE),
        (hot_ok, "hot_get", TARGET_HOT_GET),
        (cold_ok, "cold_get", TARGET_COLD_GET),
        (eq_ok, "eq_filter", TARGET_EQ_FILTER),
    ] {
        let actual = count as f64 / total_ok as f64;
        if (actual - target).abs() > MIX_DRIFT_TOLERANCE {
            return Err(MixContractError::MixDrift {
                class,
                actual,
                target,
            });
        }
    }
    Ok(())
}

/// Drop hybrid Indra copies so the next get misses through to the SQL primary.
///
/// Returns `true` when the backend was a hybrid adapter. Missing hybrid support
/// or a non-hybrid backend returns `false` (fail closed, no panic).
#[allow(clippy::unused_async)] // hybrid cfg enables await; default workspace build does not.
pub async fn drop_hybrid_unique_mirrors(
    backend: &Arc<dyn DatabaseBackend>,
    table: &str,
    ids: &[String],
) -> Result<bool> {
    #[cfg(feature = "hybrid")]
    {
        use valence_backend_hybrid::HybridBackend;
        let Some(hybrid) = backend
            .as_any_local()
            .and_then(|any| any.downcast_ref::<HybridBackend>())
        else {
            return Ok(false);
        };
        for id in ids {
            hybrid.invalidate_cached_record(table, id).await?;
        }
        Ok(true)
    }
    #[cfg(not(feature = "hybrid"))]
    {
        let _ = (backend, table, ids);
        Ok(false)
    }
}

#[allow(clippy::unused_async)] // hybrid cfg enables await; default workspace build does not.
async fn invalidate_hybrid_one(
    backend: &Arc<dyn DatabaseBackend>,
    table: &str,
    id: &str,
) -> Result<()> {
    #[cfg(feature = "hybrid")]
    {
        use valence_backend_hybrid::HybridBackend;
        if let Some(hybrid) = backend
            .as_any_local()
            .and_then(|any| any.downcast_ref::<HybridBackend>())
        {
            hybrid.invalidate_cached_record(table, id).await?;
        }
        Ok(())
    }
    #[cfg(not(feature = "hybrid"))]
    {
        let _ = (backend, table, id);
        Ok(())
    }
}

fn equality_filter(engine_id: &str, table: &str, label: &str) -> Result<CompiledQuery> {
    let core = QueryCore::new(table.to_string())
        .where_string(
            "label".to_string(),
            StringPredicate::Equals(label.to_string()),
        )
        .limit(1);
    compile_for_engine(engine_id, &core).map_err(|e| anyhow::anyhow!("{e}"))
}

fn class_result(ok: u64, errors: usize, total_ok: u64, samples: Vec<f64>) -> MixedClassResult {
    MixedClassResult {
        ops: ok,
        error_count: errors,
        share: if total_ok == 0 {
            0.0
        } else {
            ok as f64 / total_ok as f64
        },
        op_ms: MetricStats::summarize(samples),
    }
}

/// Run the `prod-mix-v1` hose for `duration_secs` at `concurrency`.
pub async fn run_mixed_oltp(
    backend: Arc<dyn DatabaseBackend>,
    table: &str,
    prefill: usize,
    duration_secs: u64,
    concurrency: usize,
) -> Result<MixedFirehoseResult> {
    ensure!(duration_secs > 0, "mixed oltp duration must be positive");
    ensure!(concurrency > 0, "mixed oltp concurrency must be positive");
    ensure!(
        prefill > HOT_SET_SIZE,
        "mixed oltp prefill must exceed the {HOT_SET_SIZE}-key hot set"
    );

    let hot = Arc::new(hot_ids(prefill));
    let cold = Arc::new(cold_ids(prefill));
    ensure!(!cold.is_empty(), "mixed oltp needs at least one cold key");

    let dropped_hybrid_mirrors = drop_hybrid_unique_mirrors(&backend, table, &cold).await?;

    let filter_label = format!("row-{}", prefill / 2);
    let compiled = Arc::new(equality_filter(backend.engine_id(), table, &filter_label)?);

    let nonce = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );

    let create_ok = Arc::new(AtomicU64::new(0));
    let hot_ok = Arc::new(AtomicU64::new(0));
    let cold_ok = Arc::new(AtomicU64::new(0));
    let eq_ok = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let seq = Arc::new(AtomicU64::new(0));
    let deadline = Instant::now() + Duration::from_secs(duration_secs);

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let backend = Arc::clone(&backend);
        let hot = Arc::clone(&hot);
        let cold = Arc::clone(&cold);
        let compiled = Arc::clone(&compiled);
        let create_ok = Arc::clone(&create_ok);
        let hot_ok = Arc::clone(&hot_ok);
        let cold_ok = Arc::clone(&cold_ok);
        let eq_ok = Arc::clone(&eq_ok);
        let errors = Arc::clone(&errors);
        let seq = Arc::clone(&seq);
        let table = table.to_string();
        let nonce = nonce.clone();
        handles.push(tokio::spawn(async move {
            let mut create_ms = Vec::new();
            let mut hot_ms = Vec::new();
            let mut cold_ms = Vec::new();
            let mut eq_ms = Vec::new();
            while Instant::now() < deadline {
                let operation = seq.fetch_add(1, Ordering::Relaxed);
                let started = Instant::now();
                let ok = match mix_op_at(operation) {
                    MixOp::Create => {
                        let id = format!("mix-{nonce}-{operation}");
                        backend
                            .create_record(&table, serde_json::json!({"id": id, "n": operation}))
                            .await
                            .is_ok()
                    }
                    MixOp::HotGet => {
                        let id = &hot[operation as usize % hot.len()];
                        matches!(backend.get_record(&table, id).await, Ok(Some(_)))
                    }
                    MixOp::ColdGet => {
                        let id = &cold[operation as usize % cold.len()];
                        let _ = invalidate_hybrid_one(&backend, &table, id).await;
                        matches!(backend.get_record(&table, id).await, Ok(Some(_)))
                    }
                    MixOp::EqFilter => {
                        match backend.execute_compiled_query(compiled.as_ref()).await {
                            Ok(rows) => !rows.is_empty(),
                            Err(_) => false,
                        }
                    }
                };
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                if ok {
                    match mix_op_at(operation) {
                        MixOp::Create => {
                            create_ok.fetch_add(1, Ordering::Relaxed);
                            create_ms.push(elapsed_ms);
                        }
                        MixOp::HotGet => {
                            hot_ok.fetch_add(1, Ordering::Relaxed);
                            hot_ms.push(elapsed_ms);
                        }
                        MixOp::ColdGet => {
                            cold_ok.fetch_add(1, Ordering::Relaxed);
                            cold_ms.push(elapsed_ms);
                        }
                        MixOp::EqFilter => {
                            eq_ok.fetch_add(1, Ordering::Relaxed);
                            eq_ms.push(elapsed_ms);
                        }
                    }
                } else {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            (create_ms, hot_ms, cold_ms, eq_ms)
        }));
    }

    let mut create_ms = Vec::new();
    let mut hot_ms = Vec::new();
    let mut cold_ms = Vec::new();
    let mut eq_ms = Vec::new();
    for handle in handles {
        let (c, h, k, e) = handle.await?;
        create_ms.extend(c);
        hot_ms.extend(h);
        cold_ms.extend(k);
        eq_ms.extend(e);
    }

    let create_ok = create_ok.load(Ordering::Relaxed);
    let hot_ok = hot_ok.load(Ordering::Relaxed);
    let cold_ok = cold_ok.load(Ordering::Relaxed);
    let eq_ok = eq_ok.load(Ordering::Relaxed);
    let error_count = errors.load(Ordering::Relaxed);
    let total_ops = create_ok + hot_ok + cold_ok + eq_ok;
    let attempts = total_ops.saturating_add(error_count as u64);
    let error_rate = if attempts == 0 {
        0.0
    } else {
        error_count as f64 / attempts as f64
    };
    let elapsed = duration_secs as f64;

    Ok(MixedFirehoseResult {
        achieved_mixed_ops_per_sec: total_ops as f64 / elapsed.max(f64::EPSILON),
        total_ops,
        error_count,
        error_rate,
        duration_secs: elapsed,
        create: class_result(create_ok, 0, total_ops, create_ms),
        hot_get: class_result(hot_ok, 0, total_ops, hot_ms),
        cold_get: class_result(cold_ok, 0, total_ops, cold_ms),
        eq_filter: class_result(eq_ok, 0, total_ops, eq_ms),
        dropped_hybrid_mirrors,
    })
}

/// Apply mix gates; `bail!` with the contract error when they fail.
pub fn ensure_mix_contract(result: &MixedFirehoseResult) -> Result<()> {
    validate_mix(
        result.create.ops,
        result.hot_get.ops,
        result.cold_get.ops,
        result.eq_filter.ops,
        result.error_count,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    if result.total_ops == 0 {
        bail!("bm-v29 produced zero successful mixed ops");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use valence_backend_mem::InMemoryBackend;

    #[test]
    fn prod_mix_v1_schedule_is_10_55_10_25() {
        let mut create = 0usize;
        let mut hot = 0usize;
        let mut cold = 0usize;
        let mut eq = 0usize;
        for i in 0..1_000u64 {
            match mix_op_at(i) {
                MixOp::Create => create += 1,
                MixOp::HotGet => hot += 1,
                MixOp::ColdGet => cold += 1,
                MixOp::EqFilter => eq += 1,
            }
        }
        assert_eq!(create, 100);
        assert_eq!(hot, 550);
        assert_eq!(cold, 100);
        assert_eq!(eq, 250);
        assert_eq!(
            MIX_CYCLE.iter().filter(|op| **op == MixOp::Create).count(),
            2
        );
        assert_eq!(
            MIX_CYCLE.iter().filter(|op| **op == MixOp::HotGet).count(),
            11
        );
        assert_eq!(
            MIX_CYCLE.iter().filter(|op| **op == MixOp::ColdGet).count(),
            2
        );
        assert_eq!(
            MIX_CYCLE
                .iter()
                .filter(|op| **op == MixOp::EqFilter)
                .count(),
            5
        );
    }

    #[test]
    fn all_operation_classes_present_in_one_cycle() {
        let seen: Vec<MixOp> = (0..20).map(mix_op_at).collect();
        assert!(seen.contains(&MixOp::Create));
        assert!(seen.contains(&MixOp::HotGet));
        assert!(seen.contains(&MixOp::ColdGet));
        assert!(seen.contains(&MixOp::EqFilter));
        validate_mix(2, 11, 2, 5, 0).expect("one cycle is on mix");
    }

    #[test]
    fn mix_tolerance_sad_path_fails_above_two_pp() {
        // 50/1000 = 5% create vs 10% target → 5pp drift.
        let err = validate_mix(50, 550, 100, 300, 0).expect_err("drift");
        match err {
            MixContractError::MixDrift { class, actual, .. } => {
                assert_eq!(class, "create");
                assert!((actual - 0.05).abs() < f64::EPSILON);
            }
            other => panic!("expected MixDrift, got {other:?}"),
        }
    }

    #[test]
    fn missing_class_and_error_rate_fail_closed() {
        let missing = validate_mix(10, 55, 0, 25, 0).expect_err("cold missing");
        assert!(matches!(
            missing,
            MixContractError::MissingClass { class: "cold_get" }
        ));
        let rate = validate_mix(100, 550, 100, 249, 2).expect_err("0.2% errors");
        match rate {
            MixContractError::ErrorRate { actual } => {
                assert!(actual >= MAX_ERROR_RATE);
            }
            other => panic!("expected ErrorRate, got {other:?}"),
        }
    }

    #[test]
    fn cold_ids_exclude_hot_set() {
        let hot = hot_ids(128);
        let cold = cold_ids(128);
        assert_eq!(hot.len(), HOT_SET_SIZE);
        assert_eq!(cold.len(), 128 - HOT_SET_SIZE);
        for id in &hot {
            assert!(!cold.contains(id), "{id} leaked into cold set");
        }
    }

    #[tokio::test]
    async fn mixed_oltp_mem_happy() {
        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        crate::workload::prefill::prefill_table(Arc::clone(&backend), "mix_unit", 128)
            .await
            .expect("prefill");
        let result = run_mixed_oltp(backend, "mix_unit", 128, 1, 2)
            .await
            .expect("mixed");
        ensure_mix_contract(&result).expect("contract");
        assert!(result.achieved_mixed_ops_per_sec > 0.0);
        assert!(result.create.ops > 0);
        assert!(result.hot_get.ops > 0);
        assert!(result.cold_get.ops > 0);
        assert!(result.eq_filter.ops > 0);
        assert!(!result.dropped_hybrid_mirrors);
    }

    #[tokio::test]
    async fn mixed_oltp_rejects_zero_bounds_sad() {
        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        let error = run_mixed_oltp(backend, "mix_unit", 128, 0, 0)
            .await
            .expect_err("zero bounds");
        assert!(error.to_string().contains("duration must be positive"));
    }
}
