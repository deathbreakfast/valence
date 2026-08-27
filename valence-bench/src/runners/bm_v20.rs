//! bm-v20: get-by-id hammer (hot key + unique keys).
//!
//! Unique gets disable the process-wide ORM read cache (`VALENCE_READ_CACHE=0`).
//! Hybrid write-through also fills IndraDB, so unique ids are dropped from the
//! mirror before that loop; otherwise unique p95 is another hot Indra hit.

use std::time::Instant;

use anyhow::Result;

use crate::report::BenchReport;
use crate::runners::RunContext;
use crate::stats::MetricStats;
use crate::workload::mixed::drop_hybrid_unique_mirrors;

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    if !crate::runners::store_available(ctx) {
        return skipped(ctx);
    }

    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;
    let backend = valence.active_backend()?;

    // Isolate from prior adapters on a shared store (hybrid reuses the postgres primary):
    // clear the fixed ids so seeding does not collide on the primary key.
    let _ = backend.delete_record("bm_v20", "hot").await;
    for i in 0..ctx.plan.default_ops.max(1) {
        let _ = backend.delete_record("bm_v20", &format!("u{i}")).await;
    }

    backend
        .create_record("bm_v20", serde_json::json!({"id": "hot", "n": 0}))
        .await?;
    let unique_n = ctx.plan.default_ops.min(200);
    let unique_ids = (0..unique_n).map(|i| format!("u{i}")).collect::<Vec<_>>();
    for (i, id) in unique_ids.iter().enumerate() {
        backend
            .create_record("bm_v20", serde_json::json!({"id": id, "n": i}))
            .await?;
    }

    let mut hot = Vec::with_capacity(ctx.plan.default_ops);
    for _ in 0..ctx.plan.default_ops {
        let start = Instant::now();
        let _ = backend.get_record("bm_v20", "hot").await?;
        hot.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let dropped_hybrid_mirrors =
        drop_hybrid_unique_mirrors(&backend, "bm_v20", &unique_ids).await?;

    std::env::set_var("VALENCE_READ_CACHE", "0");
    let mut cold = Vec::with_capacity(unique_n);
    for id in &unique_ids {
        let start = Instant::now();
        let _ = backend.get_record("bm_v20", id).await?;
        cold.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    std::env::remove_var("VALENCE_READ_CACHE");

    let hot_stats = MetricStats::summarize(hot);
    let cold_stats = MetricStats::summarize(cold);
    let unique_note = if dropped_hybrid_mirrors {
        "unique/primary-miss"
    } else {
        "unique/cache-off"
    };
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.ops = Some(ctx.plan.default_ops);
    report.op_ms = Some(hot_stats);
    report.query_ms = Some(cold_stats);
    report.pass_notes = Some(format!(
        "hot get p95 {:.3} ms; {unique_note} p95 {:.3} ms",
        hot_stats.p95, cold_stats.p95
    ));
    Ok(report)
}

fn skipped(ctx: &RunContext) -> Result<BenchReport> {
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.status = "skipped";
    report.pass_notes = crate::runners::store_skip_reason(ctx);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::resolve_experiment;
    use crate::matrix::matrix_from_cli;
    use crate::sweep::SweepParams;
    use valence_testkit::WireBackendOptions;

    fn sqlite_context() -> RunContext {
        RunContext {
            matrix: matrix_from_cli("sqlite", "off", "embedded").expect("matrix"),
            plan: resolve_experiment("bm-v20", Some(8)).expect("experiment"),
            warmup: 0,
            sweep: SweepParams::default(),
            wire: WireBackendOptions::default(),
        }
    }

    #[tokio::test]
    async fn bm_v20_sqlite_notes_cache_off_unique_happy() {
        let report = run(&sqlite_context()).await.expect("bm-v20 sqlite");
        assert_eq!(report.status, "ok");
        let notes = report.pass_notes.expect("notes");
        assert!(notes.contains("unique/cache-off"), "{notes}");
        assert!(!notes.contains("unique/primary-miss"), "{notes}");
        assert!(report.query_ms.is_some());
    }

    #[tokio::test]
    async fn bm_v20_hybrid_unavailable_skips_sad() {
        let ctx = RunContext {
            matrix: matrix_from_cli("hybrid", "off", "embedded").expect("matrix"),
            plan: resolve_experiment("bm-v20", Some(8)).expect("experiment"),
            warmup: 0,
            sweep: SweepParams::default(),
            wire: WireBackendOptions::default(),
        };
        assert!(!crate::runners::store_available(&ctx));
        let report = run(&ctx).await.expect("skip");
        assert_eq!(report.status, "skipped");
        assert!(report.query_ms.is_none());
    }
}
