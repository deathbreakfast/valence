//! bm-v31: synchronous DAG `delete_entity_now` latency by fan-out (RQ-VW6).
//!
//! ```text
//! cargo run -p valence-bench --release -- \
//!   run --experiment bm-v31 --storage mem --ops 200 --prefill 8
//! ```
//!
//! Prefill is treated as child fan-out per root (not row count). Each op creates
//! one parent + `prefill` cascade children, then measures `delete_entity_now`.

use std::time::Instant;

use anyhow::Result;
use valence_core::deletion::delete_entity_now;
use valence_core::evaluator::{DatabaseEvaluator, DEFAULT_IN_MEMORY};
use valence_core::privacy_policies::common::PUBLIC_READ;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaConnection, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};

use crate::report::BenchReport;
use crate::runners::RunContext;
use crate::stats::MetricStats;

const SCENARIO_ID: &str = "delete_now_fanout";
const ROOT: &str = "bm_v31_root";
const CHILD: &str = "bm_v31_child";

fn leak_schema(schema: Schema) -> &'static Schema {
    Box::leak(Box::new(schema))
}

fn public_cascade_root() -> &'static SchemaMetadata {
    let schema = leak_schema(Schema {
        name: ROOT.into(),
        version: "0.1.0".into(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "t".into(),
            write: "t".into(),
        },
        policies: Some(SchemaPolicies {
            delete: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: "PUBLIC".into(),
                    description: None,
                    evaluator: Some(&PUBLIC_READ),
                }],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![],
        edges: Vec::new(),
        connections: vec![SchemaConnection {
            name: "kids".into(),
            from_table: ROOT.into(),
            from_field: "id".into(),
            to_table: CHILD.into(),
            cardinality: "HasMany".into(),
            required: false,
            on_delete: "Cascade".into(),
            label: "kids".into(),
            model_path: None,
            reverse_field: Some("parent_id".into()),
            edge_table: None,
            target_trait: None,
        }],
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "1d".into(),
            row_count: 0,
            owner: "t".into(),
            description: None,
        },
    });
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

fn public_child() -> &'static SchemaMetadata {
    let schema = leak_schema(Schema {
        name: CHILD.into(),
        version: "0.1.0".into(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "t".into(),
            write: "t".into(),
        },
        policies: Some(SchemaPolicies {
            delete: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: "PUBLIC".into(),
                    description: None,
                    evaluator: Some(&PUBLIC_READ),
                }],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![],
        edges: Vec::new(),
        connections: vec![],
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "1d".into(),
            row_count: 0,
            owner: "t".into(),
            description: None,
        },
    });
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_cascade_root())
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_child())
}

fn ensure_schemas() {
    let reg = SchemaRegistry::global();
    assert!(
        reg.get_schema(ROOT).is_some() && reg.get_schema(CHILD).is_some(),
        "bm-v31 schemas missing from SchemaRegistry::global"
    );
}

pub async fn run(ctx: &RunContext) -> Result<BenchReport> {
    if !crate::runners::store_available(ctx) {
        let reason = crate::runners::store_skip_reason(ctx).unwrap_or_default();
        let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix);
        report.status = "skipped";
        report.scenario_id = Some(SCENARIO_ID.to_string());
        report.pass_notes = Some(reason);
        return Ok(report);
    }

    ensure_schemas();
    let fanout = ctx.sweep.prefill.max(1);
    let mut session = crate::runners::bootstrap_session(ctx);
    session.spawn().await?;
    let valence = session.ensure_valence()?;
    let backend = valence.active_backend()?;

    let mut samples = Vec::with_capacity(ctx.plan.default_ops);
    for i in 0..ctx.plan.default_ops {
        let pid = format!("p{i}");
        backend
            .create_record(ROOT, serde_json::json!({"id": pid, "name": "root"}))
            .await?;
        for c in 0..fanout {
            let cid = format!("c{i}_{c}");
            backend
                .create_record(
                    CHILD,
                    serde_json::json!({
                        "id": cid,
                        "parent_id": format!("{ROOT}:{pid}")
                    }),
                )
                .await?;
        }
        let start = Instant::now();
        delete_entity_now(ROOT, &pid, valence).await?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let stats = MetricStats::summarize(samples);
    let mut report = BenchReport::base(&ctx.plan.id, &ctx.matrix).with_sweep(&ctx.sweep);
    report.ops = Some(ctx.plan.default_ops);
    report.op_ms = Some(stats);
    report.scenario_id = Some(SCENARIO_ID.to_string());
    report.prefill_count = Some(fanout);
    report.pass_notes = Some(format!(
        "delete_entity_now fanout={fanout} p95 {:.3} ms (RQ-VW6)",
        stats.p95
    ));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::resolve_experiment;
    use crate::matrix::matrix_from_cli;
    use crate::sweep::SweepParams;
    use valence_testkit::WireBackendOptions;

    #[tokio::test]
    async fn bm_v31_mem_smoke() {
        let ctx = RunContext {
            matrix: matrix_from_cli("mem", "off", "embedded").expect("matrix"),
            plan: resolve_experiment("bm-v31", Some(4)).expect("experiment"),
            warmup: 0,
            sweep: SweepParams {
                prefill: 2,
                ..SweepParams::default()
            },
            wire: WireBackendOptions::default(),
        };
        let report = run(&ctx).await.expect("bm-v31");
        assert_eq!(report.status, "ok");
        assert_eq!(report.ops, Some(4));
        assert!(report.op_ms.is_some());
    }
}
