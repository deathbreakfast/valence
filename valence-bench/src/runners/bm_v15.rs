//! bm-v15: delegates to hop pair harness (replaces weak single-adapter smoke).

use anyhow::Result;
use valence_testkit::{run_hop_pair_contract, HopPair, StorageAdapter};

use crate::report::BenchReport;
use crate::runners::RunContext;

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    let pair = HopPair {
        primary: StorageAdapter::Mem,
        secondary: StorageAdapter::Sqlite,
    };
    let start = std::time::Instant::now();
    run_hop_pair_contract(pair, Some(&ctx.wire)).await?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.pass_notes = Some(format!(
        "hop pair {} wall {elapsed:.3} ms (see bm-v24/v25 for matrix)",
        pair.slug()
    ));
    Ok(report)
}
