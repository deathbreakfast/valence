//! bm-v27: ORM query privacy post-filter overhead (on vs `VALENCE_PRIVACY_BYPASS=1`).

use std::time::Instant;

use anyhow::Result;
use product_model_host::Project;
use valence_core::Model;

use crate::report::BenchReport;
use crate::runners::RunContext;
use crate::stats::MetricStats;

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    if !crate::runners::store_available(ctx) {
        let reason = crate::runners::store_skip_reason(ctx).unwrap_or_default();
        let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
        report.status = "skipped";
        report.pass_notes = Some(reason);
        return Ok(report);
    }

    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
    let depth = ctx.sweep.prefill.max(100);
    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;

    for i in 0..depth {
        let project = Project::new(format!("privacy-q-{i}")).expect("new");
        Project::create(project, valence).await?;
    }

    std::env::set_var("VALENCE_PRIVACY_BYPASS", "0");
    let mut with_filter = Vec::with_capacity(ctx.sweep.query_iters);
    for _ in 0..ctx.warmup {
        let _ = Project::query(valence).await?;
    }
    for _ in 0..ctx.sweep.query_iters {
        let start = Instant::now();
        let _ = Project::query(valence).await?;
        with_filter.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    std::env::set_var("VALENCE_PRIVACY_BYPASS", "1");
    let mut bypass = Vec::with_capacity(ctx.sweep.query_iters);
    for _ in 0..ctx.sweep.query_iters {
        let start = Instant::now();
        let _ = Project::query(valence).await?;
        bypass.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    std::env::set_var("VALENCE_PRIVACY_BYPASS", "0");

    let filter_stats = MetricStats::summarize(with_filter);
    let bypass_stats = MetricStats::summarize(bypass);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.prefill_count = Some(depth);
    report.query_ms = Some(filter_stats);
    report.pass_notes = Some(format!(
        "query privacy post-filter p95 {:.3} ms vs bypass p95 {:.3} ms @ prefill={depth}",
        filter_stats.p95, bypass_stats.p95
    ));
    Ok(report)
}
