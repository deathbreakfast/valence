//! bm-v28: sustained get-by-id firehose for marketing capacity rows.
//!
//! Run with:
//!
//! ```text
//! cargo run -p valence-bench --release -- run --experiment bm-v28 \
//!   --storage sqlite --duration-secs 30 --concurrency 32
//! ```

use std::sync::Arc;

use anyhow::Result;

use crate::report::{BenchReport, ReadMetrics};
use crate::runners::RunContext;
use crate::workload::firehose::run_read_firehose;

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    if !crate::runners::store_available(ctx) {
        return Ok(skipped(ctx));
    }

    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;
    let backend: Arc<dyn valence_core::DatabaseBackend> = valence.active_backend()?;
    let firehose = run_read_firehose(
        backend,
        "bm_v28",
        ctx.sweep.duration_secs,
        ctx.sweep.concurrency,
    )
    .await?;

    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.read = Some(ReadMetrics {
        achieved_read_ops_per_sec: firehose.achieved_read_ops_per_sec,
        error_rate: firehose.error_rate,
        total_ops: firehose.total_ops,
        error_count: firehose.error_count,
        op_ms: firehose.op_ms,
    });
    report.op_ms = Some(firehose.op_ms);
    report.ops_per_sec = Some(firehose.achieved_read_ops_per_sec);
    report.error_rate = Some(firehose.error_rate);
    report.pass_notes = Some(format!(
        "read firehose {:.1} ops/s p95 {:.3} ms error_rate {:.4}",
        firehose.achieved_read_ops_per_sec, firehose.op_ms.p95, firehose.error_rate
    ));
    Ok(report)
}

fn skipped(ctx: &RunContext) -> BenchReport {
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.status = "skipped";
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
            plan: resolve_experiment("bm-v28", None).expect("experiment"),
            warmup: 0,
            sweep: SweepParams {
                duration_secs: 1,
                concurrency: 2,
                ..SweepParams::default()
            },
            wire: WireBackendOptions::default(),
        }
    }

    async fn assert_firehose_report(storage: &str) {
        let report = run(&context(storage)).await.expect("bm-v28 report");
        assert_eq!(report.status, "ok");
        assert!(report.ops_per_sec.is_some_and(|rate| rate > 0.0));
        assert_eq!(report.error_rate, Some(0.0));
        let read = report.read.expect("read metrics");
        assert!(read.total_ops > 0);
        assert!(read.op_ms.count > 0);
    }

    #[tokio::test]
    async fn bm_v28_read_firehose_mem_happy() {
        assert_firehose_report("mem").await;
    }

    #[tokio::test]
    async fn bm_v28_read_firehose_sqlite_happy() {
        assert_firehose_report("sqlite").await;
    }

    #[tokio::test]
    async fn bm_v28_store_unavailable_sad() {
        let ctx = context("hybrid");
        assert!(!crate::runners::store_available(&ctx));
        let report = run(&ctx).await.expect("unavailable store report");
        assert_eq!(report.status, "skipped");
        assert!(report.pass_notes.is_some());
        assert!(report.ops_per_sec.is_none());
        assert!(report.read.is_none());
    }
}
