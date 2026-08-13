//! bm-v20: get-by-id hammer (hot key + unique keys; cache on/off).

use std::time::Instant;

use anyhow::Result;

use crate::report::BenchReport;
use crate::runners::RunContext;
use crate::stats::MetricStats;

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
    for i in 0..ctx.plan.default_ops.max(1) {
        backend
            .create_record("bm_v20", serde_json::json!({"id": format!("u{i}"), "n": i}))
            .await?;
    }

    let mut hot = Vec::with_capacity(ctx.plan.default_ops);
    for _ in 0..ctx.plan.default_ops {
        let start = Instant::now();
        let _ = backend.get_record("bm_v20", "hot").await?;
        hot.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    std::env::set_var("VALENCE_READ_CACHE", "0");
    let mut cold = Vec::with_capacity(ctx.plan.default_ops.min(200));
    for i in 0..ctx.plan.default_ops.min(200) {
        let id = format!("u{i}");
        let start = Instant::now();
        let _ = backend.get_record("bm_v20", &id).await?;
        cold.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    std::env::remove_var("VALENCE_READ_CACHE");

    let hot_stats = MetricStats::summarize(hot);
    let cold_stats = MetricStats::summarize(cold);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.ops = Some(ctx.plan.default_ops);
    report.op_ms = Some(hot_stats);
    report.query_ms = Some(cold_stats);
    report.pass_notes = Some(format!(
        "hot get p95 {:.3} ms; unique/cache-off p95 {:.3} ms",
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
