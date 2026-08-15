//! Experiment runners (bm-v0..bm-v30).

mod bm_v0;
mod bm_v1;
mod bm_v11;
mod bm_v12;
mod bm_v13;
mod bm_v14;
mod bm_v15;
mod bm_v16;
mod bm_v17;
mod bm_v18;
mod bm_v19;
mod bm_v2;
mod bm_v20;
mod bm_v21;
mod bm_v22;
mod bm_v23;
mod bm_v24;
mod bm_v25;
mod bm_v26;
mod bm_v27;
mod bm_v28;
mod bm_v29;
mod bm_v3;
mod bm_v30;
mod bm_v4;
mod bm_v5;
mod bm_v6;
mod bm_v7;
mod bm_v8;
mod bm_v9;

use anyhow::Result;
use valence_testkit::{extended_store_available_with_wire, extended_store_skip_reason_with_wire};
use valence_testkit::{BootstrapSession, MatrixSpec, WireBackendOptions};

use crate::experiments::ExperimentPlan;
use crate::report::BenchReport;
use crate::sweep::SweepParams;

pub struct RunContext {
    pub matrix: MatrixSpec,
    pub plan: ExperimentPlan,
    pub warmup: usize,
    pub sweep: SweepParams,
    pub wire: WireBackendOptions,
}

pub fn store_available(ctx: &RunContext) -> bool {
    extended_store_available_with_wire(ctx.matrix.storage, Some(&ctx.wire))
}

pub fn store_skip_reason(ctx: &RunContext) -> Option<String> {
    extended_store_skip_reason_with_wire(ctx.matrix.storage, Some(&ctx.wire))
}

pub fn bootstrap_session(ctx: &RunContext) -> BootstrapSession {
    BootstrapSession::new(ctx.matrix).with_wire_options(ctx.wire.clone())
}

pub async fn run_experiment(ctx: &RunContext) -> Result<BenchReport> {
    if let Some(report) = run_legacy(ctx).await? {
        return Ok(report);
    }
    if let Some(report) = run_real_world(ctx).await? {
        return Ok(report);
    }
    anyhow::bail!("no runner for {}", ctx.plan.id)
}

async fn run_legacy(ctx: &RunContext) -> Result<Option<BenchReport>> {
    let report = match ctx.plan.id.as_str() {
        "bm-v0" => bm_v0::run(ctx).await?,
        "bm-v1" => bm_v1::run(ctx).await?,
        "bm-v2" => bm_v2::run(ctx).await?,
        "bm-v3" => bm_v3::run(ctx).await?,
        "bm-v4" => bm_v4::run(ctx).await?,
        "bm-v5" => bm_v5::run(ctx).await?,
        "bm-v6" => bm_v6::run(ctx).await?,
        "bm-v7" => bm_v7::run(ctx).await?,
        "bm-v8" => bm_v8::run(ctx).await?,
        "bm-v9" => bm_v9::run(ctx).await?,
        "bm-v11" => bm_v11::run(ctx).await?,
        "bm-v12" => bm_v12::run(ctx).await?,
        "bm-v13" => bm_v13::run(ctx).await?,
        "bm-v14" => bm_v14::run(ctx).await?,
        _ => return Ok(None),
    };
    Ok(Some(report))
}

async fn run_real_world(ctx: &RunContext) -> Result<Option<BenchReport>> {
    let report = match ctx.plan.id.as_str() {
        "bm-v15" => bm_v15::run(ctx).await?,
        "bm-v16" => bm_v16::run(ctx).await?,
        "bm-v17" => bm_v17::run(ctx).await?,
        "bm-v18" => bm_v18::run(ctx).await?,
        "bm-v19" => bm_v19::run(ctx).await?,
        "bm-v20" => bm_v20::run(ctx).await?,
        "bm-v21" => bm_v21::run(ctx).await?,
        "bm-v22" => bm_v22::run(ctx).await?,
        "bm-v23" => bm_v23::run(ctx).await?,
        "bm-v24" => bm_v24::run(ctx).await?,
        "bm-v25" => bm_v25::run(ctx).await?,
        "bm-v26" => bm_v26::run(ctx).await?,
        "bm-v27" => bm_v27::run(ctx).await?,
        "bm-v28" => bm_v28::run(ctx).await?,
        "bm-v29" => bm_v29::run(ctx).await?,
        "bm-v30" => bm_v30::run(ctx).await?,
        _ => return Ok(None),
    };
    Ok(Some(report))
}
