//! bm-v17: privacy eval sleep sensitivity.

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
    std::env::set_var("VALENCE_PRIVACY_BYPASS", "0");
    std::env::set_var(
        "VALENCE_PRIVACY_SLEEP_US",
        ctx.sweep.privacy_sleep_us.to_string(),
    );

    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;

    let project = Project::new("sleep-bench".to_string()).expect("new");
    let created = Project::create_used(project, valence, valence::use_!(r"**Test:** Seeds fixture **projects** for a Valence bench runner so throughput timing has rows to create under load. Benchmark operators running the suite only.")).await?;
    let id = created.id().expect("id").id();

    let mut samples = Vec::with_capacity(ctx.plan.default_ops);
    for _ in 0..ctx.plan.default_ops {
        let start = Instant::now();
        let _ = Project::get_used(id, valence, valence::use_!(r"**Test:** Reloads fixture **projects** by id in a Valence bench runner so read latency can be measured under load. Benchmark operators running the suite only.")).await?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let stats = MetricStats::summarize(samples);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.query_ms = Some(stats);
    report.pass_notes = Some(format!(
        "privacy sleep {} us p95 {:.3} ms",
        ctx.sweep.privacy_sleep_us, stats.p95
    ));
    Ok(report)
}
