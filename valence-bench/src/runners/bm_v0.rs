//! bm-v0: single-table create/get throughput.

use std::time::Instant;

use anyhow::Result;

use crate::report::BenchReport;
use crate::runners::RunContext;
use crate::stats::MetricStats;

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;
    let backend = valence.active_backend()?;

    // Isolate from prior adapters that share a physical store (e.g. hybrid reuses the
    // postgres primary the standalone postgres adapter already wrote): clear the ids
    // this run creates so the create-throughput measurement starts from empty rows.
    for i in 0..ctx.warmup {
        let _ = backend.delete_record("bm_v0", &format!("warm{i}")).await;
    }
    for i in 0..ctx.plan.default_ops {
        let _ = backend.delete_record("bm_v0", &format!("rec{i}")).await;
    }

    for i in 0..ctx.warmup {
        let id = format!("warm{i}");
        backend
            .create_record("bm_v0", serde_json::json!({"id": id}))
            .await?;
        backend.get_record("bm_v0", &id).await?;
    }

    let mut samples = Vec::with_capacity(ctx.plan.default_ops);
    for i in 0..ctx.plan.default_ops {
        let id = format!("rec{i}");
        let start = Instant::now();
        backend
            .create_record("bm_v0", serde_json::json!({"id": id, "i": i}))
            .await?;
        backend.get_record("bm_v0", &id).await?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let stats = MetricStats::summarize(samples);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.ops = Some(ctx.plan.default_ops);
    report.op_ms = Some(stats);
    report.ops_per_sec = Some(ctx.plan.default_ops as f64 / (stats.p50 / 1000.0).max(f64::EPSILON));
    report.pass_notes = Some(format!(
        "create+get p95 {:.3} ms over {} ops",
        stats.p95, stats.count
    ));
    Ok(report)
}
