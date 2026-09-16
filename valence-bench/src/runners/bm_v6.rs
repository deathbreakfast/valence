//! bm-v6: ORM write firehose via generated `Project::create`.

use std::time::{Duration, Instant};

use anyhow::Result;
use product_model_host::Project;
use valence_core::Model;

use crate::report::BenchReport;
use crate::runners::RunContext;

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

    let deadline = Instant::now() + Duration::from_secs(ctx.sweep.duration_secs);
    let mut ok = 0u64;
    let mut n = 0usize;
    while Instant::now() < deadline {
        let project = Project::new(format!("bench-{n}")).expect("new");
        Project::create_used(project, valence, valence::use_!(r"**Test:** Seeds fixture **projects** for a Valence bench runner so throughput timing has rows to create under load. Benchmark operators running the suite only.")).await?;
        ok += 1;
        n += 1;
    }

    let ops_per_sec = ok as f64 / ctx.sweep.duration_secs as f64;
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.ops = Some(ok as usize);
    report.ops_per_sec = Some(ops_per_sec);
    report.pass_notes = Some(format!("ORM create firehose {ops_per_sec:.1} ops/s"));
    Ok(report)
}
