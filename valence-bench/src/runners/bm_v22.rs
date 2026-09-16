//! bm-v22: full-scan / large-N unfiltered ORM list.

use anyhow::Result;
use product_model_host::Project;
use valence_core::Model;

use crate::report::BenchReport;
use crate::runners::RunContext;
use crate::stats::MetricStats;

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    if !crate::runners::store_available(ctx) {
        return skipped(ctx);
    }

    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
    let depth = ctx.sweep.prefill;
    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;

    for i in 0..depth {
        let project = Project::new(format!("scan-{i}")).expect("new");
        Project::create_used(project, valence, valence::use_!(r"**Test:** Seeds fixture **projects** for a Valence bench runner so throughput timing has rows to create under load. Benchmark operators running the suite only.")).await?;
    }

    let mut samples = Vec::with_capacity(ctx.sweep.query_iters.min(50));
    let mut last_len = 0usize;
    for _ in 0..ctx.sweep.query_iters.min(50) {
        let start = std::time::Instant::now();
        let rows = Project::query_used(valence, valence::use_!(r"**Test:** Queries fixture **projects** in a Valence bench runner so filter and scan latency can be measured under load. Benchmark operators running the suite only.")).await?;
        last_len = rows.len();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let stats = MetricStats::summarize(samples);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.prefill_count = Some(depth);
    report.query_ms = Some(stats);
    report.pass_notes = Some(format!(
        "full scan rows={last_len} @ prefill={depth} p95 {:.3} ms",
        stats.p95
    ));
    Ok(report)
}

fn skipped(ctx: &RunContext) -> Result<BenchReport> {
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.status = "skipped";
    report.pass_notes = crate::runners::store_skip_reason(ctx);
    Ok(report)
}
