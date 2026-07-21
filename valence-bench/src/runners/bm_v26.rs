//! bm-v26: hybrid IndraDB+Postgres vs postgres vs indradb (get / query / hop).

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use valence_core::{CompiledQuery, DatabaseBackend, RecordId};
use valence_testkit::StorageAdapter;

use crate::report::BenchReport;
use crate::runners::RunContext;
use crate::stats::MetricStats;
use crate::workload::prefill::prefill_table;
use crate::workload::query_bench::run_query_loop;

/// Run get-by-id, compiled query, and M2M hop tracks for the matrix storage.
pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    if !matches!(
        ctx.matrix.storage,
        StorageAdapter::HybridIndraPg | StorageAdapter::Postgres | StorageAdapter::IndraDb
    ) {
        let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
        report.status = "skipped";
        report.pass_notes = Some(
            "bm-v26 compares hybrid|postgres|indradb only — pass --storage hybrid,postgres,indradb"
                .into(),
        );
        return Ok(report);
    }
    if !crate::runners::store_available(ctx) {
        let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
        report.status = "skipped";
        report.pass_notes = crate::runners::store_skip_reason(ctx);
        return Ok(report);
    }

    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;
    let backend = valence.active_backend()?;

    // Namespace tables by storage so hybrid/postgres share one Postgres without collisions.
    let ns = ctx.matrix.storage.slug().replace('-', "_");
    let get_p95 = measure_get_hammer(Arc::clone(&backend), &ns, ctx.plan.default_ops).await?;
    let query_p95 = measure_compiled_query(Arc::clone(&backend), &ns, ctx).await?;
    let hop_p95 = measure_m2m_hop(Arc::clone(&backend), &ns, ctx.plan.default_ops.min(200)).await?;

    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
    report.ops = Some(ctx.plan.default_ops);
    report.op_ms = Some(get_p95);
    report.query_ms = Some(query_p95);
    report.pass_notes = Some(format!(
        "storage={} get_p95={:.3}ms query_p95={:.3}ms hop_p95={:.3}ms (hypothesis: hop≈indradb, query≈postgres)",
        ctx.matrix.storage.slug(),
        get_p95.p95,
        query_p95.p95,
        hop_p95.p95
    ));
    report.scenario_id = Some(format!("hybrid-compare-{}", ctx.matrix.storage.slug()));
    Ok(report)
}

/// Hot get-by-id latency after seeding one row.
async fn measure_get_hammer(
    backend: Arc<dyn DatabaseBackend>,
    ns: &str,
    ops: usize,
) -> Result<MetricStats> {
    let table = format!("bm_v26_get_{ns}");
    let _ = backend.delete_record(&table, "hot").await;
    backend
        .create_record(&table, serde_json::json!({"id": "hot", "n": 0}))
        .await?;
    let mut samples = Vec::with_capacity(ops.max(1));
    for _ in 0..ops.max(1) {
        let start = Instant::now();
        let _ = backend.get_record(&table, "hot").await?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(MetricStats::summarize(samples))
}

/// Compiled SELECT latency at a modest prefill.
async fn measure_compiled_query(
    backend: Arc<dyn DatabaseBackend>,
    ns: &str,
    ctx: &RunContext,
) -> Result<MetricStats> {
    let table = format!("bm_v26_q_{ns}");
    let depth = ctx.sweep.prefill.clamp(1_000, 10_000);
    prefill_table(Arc::clone(&backend), &table, depth).await?;
    let compiled = CompiledQuery::new(format!("SELECT * FROM {table} LIMIT 100"), vec![]);
    run_query_loop(
        backend,
        &compiled,
        ctx.sweep.query_iters.max(50),
        ctx.warmup,
    )
    .await
}

/// Same-backend M2M hop: source org → edge → project targets.
async fn measure_m2m_hop(
    backend: Arc<dyn DatabaseBackend>,
    ns: &str,
    ops: usize,
) -> Result<MetricStats> {
    let org = format!("bm_v26_org_{ns}");
    let project = format!("bm_v26_project_{ns}");
    let edge = format!("bm_v26_org_projects_{ns}");
    let _ = backend.delete_record(&org, "o1").await;
    backend
        .create_record(&org, serde_json::json!({"id": "o1", "name": "acme"}))
        .await?;
    for i in 0..32 {
        let pid = format!("p{i}");
        let _ = backend.delete_record(&project, &pid).await;
        backend
            .create_record(
                &project,
                serde_json::json!({"id": pid, "name": format!("proj{i}")}),
            )
            .await?;
        backend
            .relate_edge(
                &RecordId::new(&org, "o1"),
                &edge,
                &RecordId::new(&project, format!("p{i}")),
            )
            .await?;
    }

    let mut samples = Vec::with_capacity(ops.max(1));
    for _ in 0..ops.max(1) {
        let start = Instant::now();
        let targets = backend
            .get_edge_targets(&RecordId::new(&org, "o1"), &edge)
            .await?;
        for t in targets {
            let _ = backend.get_record(t.table(), t.id()).await?;
        }
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(MetricStats::summarize(samples))
}
