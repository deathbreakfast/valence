//! bm-v16: privacy read gate overhead (on vs bypass env).

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
    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;

    let project = Project::new("privacy-bench".to_string()).expect("new");
    let created = Project::create(project, valence).await?;
    let id = created.id().expect("id").id();

    std::env::set_var("VALENCE_PRIVACY_BYPASS", "0");
    std::env::remove_var("VALENCE_PRIVACY_BYPASS_FORCE_ON");
    let mut with_gate = Vec::with_capacity(ctx.plan.default_ops);
    for _ in 0..ctx.plan.default_ops {
        let start = Instant::now();
        let _ = Project::get(id, valence).await?;
        with_gate.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    std::env::set_var("VALENCE_PRIVACY_BYPASS", "1");
    std::env::set_var("VALENCE_PRIVACY_BYPASS_FORCE_ON", "1");
    let mut bypass = Vec::with_capacity(ctx.plan.default_ops);
    for _ in 0..ctx.plan.default_ops {
        let start = Instant::now();
        let _ = Project::get(id, valence).await?;
        bypass.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    std::env::set_var("VALENCE_PRIVACY_BYPASS", "0");
    std::env::remove_var("VALENCE_PRIVACY_BYPASS_FORCE_ON");

    let gate_stats = MetricStats::summarize(with_gate);
    let bypass_stats = MetricStats::summarize(bypass);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.ops = Some(ctx.plan.default_ops);
    report.query_ms = Some(gate_stats);
    report.pass_notes = Some(format!(
        "privacy gate p50 {:.3} ms vs bypass {:.3} ms",
        gate_stats.p50, bypass_stats.p50
    ));
    Ok(report)
}
