//! bm-v12: ORM filter query after prefill.

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
    let depth = ctx.sweep.prefill;
    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;

    for i in 0..depth {
        let project = Project::new(format!("prefill-{i}")).expect("new");
        Project::create_used(project, valence, valence::use_!("create Project in src/runners/bm_v12.rs; Valence persistence for this feature path; typed store; visible to test harness.")).await?;
    }

    let mut samples = Vec::with_capacity(ctx.sweep.query_iters);
    for _ in 0..ctx.warmup {
        let _ = Project::query_used(valence, valence::use_!("query Project in src/runners/bm_v12.rs; Valence persistence for this feature path; typed store; visible to test harness.")).await?;
    }
    for _ in 0..ctx.sweep.query_iters {
        let start = std::time::Instant::now();
        let _ = Project::query_used(valence, valence::use_!("query Project in src/runners/bm_v12.rs; Valence persistence for this feature path; typed store; visible to test harness.")).await?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let stats = MetricStats::summarize(samples);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.prefill_count = Some(depth);
    report.query_ms = Some(stats);
    report.pass_notes = Some(format!(
        "ORM query @ prefill={depth} p95 {:.3} ms",
        stats.p95
    ));
    Ok(report)
}
