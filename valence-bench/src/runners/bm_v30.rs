//! bm-v30: mixed OLTP (`prod-mix-v1`) for N=2/4 client scale cells.
//!
//! Same mix, duration, and concurrency as [`super::bm_v29`], with a per-client
//! table so concurrent app nodes do not share a write set. bm-v29 stays locked
//! to one client; raising `--bench-clients` there does not change its method.
//!
//! ```text
//! VALENCE_BENCH_CLIENT_INDEX=0 cargo run -p valence-bench --release -- \
//!   run --experiment bm-v30 --storage hybrid --duration-secs 30 \
//!   --concurrency 32 --prefill 10000 --bench-clients 2
//! ```

use std::sync::Arc;

use anyhow::Result;

use crate::report::{BenchReport, MixedClassMetrics, MixedWorkloadMetrics};
use crate::runners::RunContext;
use crate::sweep::SweepParams;
use crate::workload::mixed::{cold_ids, ensure_mix_contract, run_mixed_oltp, SCENARIO_ID};
use crate::workload::prefill::prefill_table;

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    if !crate::runners::store_available(ctx) {
        return Ok(skipped(ctx));
    }

    let client_index = SweepParams::client_index();
    let table = format!("bm_v30_bc{client_index}");
    let prefill = ctx.sweep.prefill;
    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;
    let backend: Arc<dyn valence_core::DatabaseBackend> = valence.active_backend()?;

    prefill_table(Arc::clone(&backend), &table, prefill).await?;

    std::env::set_var("VALENCE_READ_CACHE", "0");
    let mixed = run_mixed_oltp(
        backend,
        &table,
        prefill,
        ctx.sweep.duration_secs,
        ctx.sweep.concurrency,
    )
    .await;
    std::env::remove_var("VALENCE_READ_CACHE");
    let mixed = mixed?;
    ensure_mix_contract(&mixed)?;

    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.scenario_id = Some(format!("{SCENARIO_ID}_client_{client_index}"));
    report.prefill_count = Some(prefill);
    report.ops_per_sec = Some(mixed.achieved_mixed_ops_per_sec);
    report.error_rate = Some(mixed.error_rate);
    report.op_ms = Some(mixed.hot_get.op_ms);
    report.query_ms = Some(mixed.eq_filter.op_ms);
    report.mixed = Some(MixedWorkloadMetrics {
        scenario_id: SCENARIO_ID.to_string(),
        achieved_mixed_ops_per_sec: mixed.achieved_mixed_ops_per_sec,
        error_rate: mixed.error_rate,
        total_ops: mixed.total_ops,
        error_count: mixed.error_count,
        create: class_metrics(&mixed.create),
        hot_get: class_metrics(&mixed.hot_get),
        cold_get: class_metrics(&mixed.cold_get),
        eq_filter: class_metrics(&mixed.eq_filter),
    });
    let cold_note = if mixed.dropped_hybrid_mirrors {
        "cold/primary-miss"
    } else {
        "cold/cache-off"
    };
    report.pass_notes = Some(format!(
        "prod-mix-v1 client {client_index} table {table} mixed {:.1} ops/s create_p95 {:.3} hot_p95 {:.3} {cold_note} p95 {:.3} eq_p95 {:.3} error_rate {:.4} (prefill={} cold_keys={})",
        mixed.achieved_mixed_ops_per_sec,
        mixed.create.op_ms.p95,
        mixed.hot_get.op_ms.p95,
        mixed.cold_get.op_ms.p95,
        mixed.eq_filter.op_ms.p95,
        mixed.error_rate,
        prefill,
        cold_ids(prefill).len(),
    ));
    Ok(report)
}

fn class_metrics(class: &crate::workload::mixed::MixedClassResult) -> MixedClassMetrics {
    MixedClassMetrics {
        ops: class.ops,
        error_count: class.error_count,
        share: class.share,
        op_ms: class.op_ms,
    }
}

fn skipped(ctx: &RunContext) -> BenchReport {
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.status = "skipped";
    report.scenario_id = Some(SCENARIO_ID.to_string());
    report.pass_notes = crate::runners::store_skip_reason(ctx);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::resolve_experiment;
    use crate::matrix::matrix_from_cli;
    use crate::sweep::SweepParams;
    use valence_testkit::WireBackendOptions;

    fn context(storage: &str) -> RunContext {
        RunContext {
            matrix: matrix_from_cli(storage, "off", "embedded").expect("matrix"),
            plan: resolve_experiment("bm-v30", None).expect("experiment"),
            warmup: 0,
            sweep: SweepParams {
                prefill: 128,
                duration_secs: 2,
                concurrency: 2,
                bench_clients: 2,
                ..SweepParams::default()
            },
            wire: WireBackendOptions::default(),
        }
    }

    async fn assert_mixed_report(storage: &str) {
        let report = run(&context(storage)).await.expect("bm-v30 report");
        assert_eq!(report.status, "ok");
        assert!(report
            .scenario_id
            .as_deref()
            .is_some_and(|id| id.starts_with(SCENARIO_ID)));
        assert!(report.ops_per_sec.is_some_and(|rate| rate > 0.0));
        assert!(report
            .error_rate
            .is_some_and(|rate| rate < crate::workload::mixed::MAX_ERROR_RATE));
        let mixed = report.mixed.expect("mixed metrics");
        assert!(mixed.create.ops > 0);
        assert!(mixed.hot_get.ops > 0);
        assert!(mixed.cold_get.ops > 0);
        assert!(mixed.eq_filter.ops > 0);
        assert!(mixed.create.op_ms.count > 0);
        assert!(mixed.hot_get.op_ms.count > 0);
        assert!(mixed.cold_get.op_ms.count > 0);
        assert!(mixed.eq_filter.op_ms.count > 0);
    }

    #[tokio::test]
    async fn bm_v30_mixed_mem_happy() {
        assert_mixed_report("mem").await;
    }

    #[tokio::test]
    async fn bm_v30_mixed_sqlite_happy() {
        assert_mixed_report("sqlite").await;
        let report = run(&context("sqlite")).await.expect("sqlite again");
        let notes = report.pass_notes.expect("notes");
        assert!(notes.contains("cold/cache-off"), "{notes}");
        assert!(!notes.contains("cold/primary-miss"), "{notes}");
        assert!(notes.contains("table bm_v30_bc"), "{notes}");
    }

    #[tokio::test]
    async fn bm_v30_hybrid_unavailable_skips_sad() {
        let ctx = context("hybrid");
        assert!(!crate::runners::store_available(&ctx));
        let report = run(&ctx).await.expect("unavailable store report");
        assert_eq!(report.status, "skipped");
        assert!(report.pass_notes.is_some());
        assert!(report.ops_per_sec.is_none());
        assert!(report.mixed.is_none());
    }

    #[tokio::test]
    async fn bm_v30_client_index_stamps_table() {
        std::env::set_var("VALENCE_BENCH_CLIENT_INDEX", "3");
        let report = run(&context("mem")).await.expect("indexed client");
        std::env::remove_var("VALENCE_BENCH_CLIENT_INDEX");
        assert_eq!(report.status, "ok");
        assert_eq!(report.scenario_id.as_deref(), Some("prod-mix-v1_client_3"));
        let notes = report.pass_notes.expect("notes");
        assert!(notes.contains("client 3"), "{notes}");
        assert!(notes.contains("table bm_v30_bc3"), "{notes}");
    }
}
