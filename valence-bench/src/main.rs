//! Synthetic Valence benchmark CLI (bm-v0..bm-v25 experiments).

#![allow(dead_code)]
#![allow(clippy::useless_format)]
#![allow(clippy::needless_collect)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
// Benchmark harness: percentile casts and uniform Result-returning runners.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unnecessary_wraps,
    clippy::trivially_copy_pass_by_ref
)]

mod experiments;
mod matrix;
mod report;
mod resource;
mod runners;
mod stats;
mod sweep;
mod workload;

mod wire;

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use experiments::{
    experiment_storage_ok, query_real_timeout_secs, resolve_experiment, ALL_EXPERIMENT_IDS,
};
use matrix::matrix_from_cli;
use report::BenchReport;
use runners::{run_experiment, RunContext};
use sweep::{parse_prefill_sweep, SweepParams};
use valence_testkit::{
    extended_store_available_with_wire, extended_store_skip_reason_with_wire, StorageAdapter,
    WireBackendOptions,
};
use wire::WireCliArgs;

#[derive(Parser)]
#[command(name = "valence-bench", about = "Valence synthetic benchmark runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List registered experiment IDs (see EXPERIMENTS.md).
    Experiments,
    /// Run one experiment id against a matrix slice.
    Run(RunArgs),
    /// Run a pre-registered matrix slice (multiple experiments).
    Matrix(MatrixArgs),
}

#[derive(Parser)]
struct RunArgs {
    #[arg(long)]
    experiment: String,
    #[arg(long, default_value = "mem")]
    storage: String,
    #[arg(long, default_value = "off")]
    telemetry: String,
    #[arg(long, default_value = "embedded")]
    topology: String,
    #[arg(long)]
    ops: Option<usize>,
    #[arg(long, default_value_t = 0)]
    warmup: usize,
    #[arg(long, default_value_t = 10_000)]
    prefill: usize,
    #[arg(long)]
    prefill_sweep: Option<String>,
    #[arg(long, default_value_t = 30)]
    duration_secs: u64,
    #[arg(long, default_value_t = 64)]
    concurrency: usize,
    #[arg(long, default_value_t = 1)]
    bench_clients: usize,
    #[arg(long, default_value_t = 1000)]
    query_iters: usize,
    #[arg(long, default_value_t = 0)]
    privacy_sleep_us: u64,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long)]
    redis_url: Option<String>,
    #[arg(long)]
    mongodb_uri: Option<String>,
    #[arg(long)]
    postgres_url: Option<String>,
    #[arg(long)]
    redis_urls: Option<String>,
}

#[derive(Parser)]
struct MatrixArgs {
    /// Slice name: adapter-minimal, write-sweep, query-depth, overhead.
    slice: String,
    #[arg(long, default_value = "mem,sqlite")]
    storage: String,
    #[arg(long, default_value = "off")]
    telemetry: String,
    #[arg(long)]
    redis_url: Option<String>,
    #[arg(long)]
    mongodb_uri: Option<String>,
    #[arg(long)]
    postgres_url: Option<String>,
    #[arg(long)]
    redis_urls: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Experiments => {
            for id in ALL_EXPERIMENT_IDS {
                println!("{id}");
            }
        }
        Command::Run(args) => {
            let matrix = matrix_from_cli(&args.storage, &args.telemetry, &args.topology)?;
            let plan = resolve_experiment(&args.experiment, args.ops)?;
            let sweep = SweepParams {
                prefill: args.prefill,
                prefill_sweep: args.prefill_sweep.as_deref().and_then(parse_prefill_sweep),
                duration_secs: args.duration_secs,
                concurrency: args.concurrency,
                bench_clients: args.bench_clients,
                query_iters: args.query_iters,
                privacy_sleep_us: args.privacy_sleep_us,
            };
            let wire = wire_from_cli(WireCliArgs {
                redis_url: args.redis_url,
                mongodb_uri: args.mongodb_uri,
                postgres_url: args.postgres_url,
                redis_urls: args.redis_urls,
            });
            let ctx = RunContext {
                matrix,
                plan,
                warmup: args.warmup,
                sweep,
                wire,
            };

            let out = run_experiment(&ctx).await?;
            write_report(&out, args.report.as_ref(), &ctx)?;
        }
        Command::Matrix(args) => {
            let wire = wire_from_cli(WireCliArgs {
                redis_url: args.redis_url.clone(),
                mongodb_uri: args.mongodb_uri.clone(),
                postgres_url: args.postgres_url.clone(),
                redis_urls: args.redis_urls.clone(),
            });
            run_matrix_slice(&args, wire).await?;
        }
    }
    Ok(())
}

fn wire_from_cli(args: WireCliArgs) -> WireBackendOptions {
    args.into_wire_options()
}

fn write_report(out: &BenchReport, path: Option<&PathBuf>, ctx: &RunContext) -> Result<()> {
    let json = serde_json::to_string_pretty(out)?;
    let path = path
        .cloned()
        .unwrap_or_else(|| BenchReport::default_report_path(&ctx.plan.id, &ctx.matrix));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &json)?;
    println!("wrote {}", path.display());
    // Echo status for CI smoke greps (`"status": "ok"`).
    println!("\"status\": \"{}\"", out.status);
    Ok(())
}

async fn run_matrix_slice(args: &MatrixArgs, wire: WireBackendOptions) -> Result<()> {
    let storages: Vec<StorageAdapter> = args
        .storage
        .split(',')
        .filter_map(|s| StorageAdapter::parse_cli(s.trim()))
        .collect();
    if storages.is_empty() {
        bail!("no valid storage adapters in --storage");
    }

    let experiments = match args.slice.as_str() {
        "adapter-minimal" => vec!["bm-v0", "bm-v11"],
        "write-sweep" => vec!["bm-v5"],
        "query-depth" => vec!["bm-v11", "bm-v12"],
        "overhead" => vec!["bm-v16", "bm-v18"],
        "read-hammer" => vec!["bm-v20"],
        "query-real" => vec!["bm-v21", "bm-v22", "bm-v23"],
        "hop-pairs" => vec!["bm-v24"],
        "hop-chains" => vec!["bm-v25"],
        other => bail!("unknown matrix slice: {other}"),
    };

    for storage in storages {
        if !extended_store_available_with_wire(storage, Some(&wire)) {
            if let Some(reason) = extended_store_skip_reason_with_wire(storage, Some(&wire)) {
                eprintln!("skip {}: {reason}", storage.slug());
            }
            continue;
        }
        let matrix = matrix_from_cli(storage.slug(), &args.telemetry, "embedded")?;
        for exp in &experiments {
            if !experiment_storage_ok(exp, storage) {
                eprintln!("skip {} @ {}: mem-only experiment", exp, storage.slug());
                continue;
            }
            let plan = resolve_experiment(exp, None)?;
            let mut sweep = SweepParams::default();
            if *exp == "bm-v11" || *exp == "bm-v12" {
                sweep.prefill = if *exp == "bm-v12" { 1_000 } else { 10_000 };
            }
            if *exp == "bm-v5" {
                sweep.duration_secs = 10;
                sweep.concurrency = 32;
            }
            let ctx = RunContext {
                matrix,
                plan,
                warmup: 50,
                sweep,
                wire: wire.clone(),
            };
            let out = match run_experiment_with_optional_timeout(&ctx).await {
                Ok(report) => report,
                Err(e) => {
                    eprintln!("error {} @ {}: {e:#}", ctx.plan.id, storage.slug());
                    let mut report = crate::report::BenchReport::base(&ctx.plan.id, &matrix);
                    report.status = "error";
                    report.pass_notes = Some(e.to_string());
                    report
                }
            };
            write_report(&out, None, &ctx)?;
        }
    }
    Ok(())
}

async fn run_experiment_with_optional_timeout(ctx: &RunContext) -> Result<BenchReport> {
    let Some(secs) = query_real_timeout_secs(&ctx.plan.id) else {
        return run_experiment(ctx).await;
    };
    if let Ok(result) =
        tokio::time::timeout(std::time::Duration::from_secs(secs), run_experiment(ctx)).await
    {
        result
    } else {
        let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
        report.status = "error";
        report.pass_notes = Some(format!(
            "wall-clock timeout after {secs}s (query-real cell abandoned to avoid hang)"
        ));
        Ok(report)
    }
}
