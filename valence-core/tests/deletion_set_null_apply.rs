//! TM-S5 / TM-S1 / TM-S8 / TM-S9 — SetNull / RemoveEdge apply + privacy filter.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::deletion::dag::{DeletionAction, DeletionDag, DeletionNode};
use valence_core::deletion::{apply_deletion_node, check_dag_delete_privacy_with_registry};
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy_policies::common::{PUBLIC_READ, SYSTEM_ONLY};
use valence_core::record_id::RecordId;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaConnection, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
use valence_core::{DatabaseBackend, DatabaseEvaluator, Valence};

fn leak_schema(schema: Schema) -> &'static Schema {
    Box::leak(Box::new(schema))
}

fn base_schema(name: &str, connections: Vec<SchemaConnection>) -> &'static SchemaMetadata {
    let schema = leak_schema(Schema {
        name: name.to_string(),
        version: "0.1.0".to_string(),
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
            update: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: "SYSTEM_ONLY".into(),
                    description: None,
                    evaluator: Some(&SYSTEM_ONLY),
                }],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![],
        edges: Vec::new(),
        connections,
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

fn mem_valence() -> Valence {
    Valence::builder()
        .add_backend(
            "default",
            Arc::new(InMemoryBackend::new()) as Arc<dyn DatabaseBackend>,
        )
        .with_actor(Actor::User {
            user_id: "u1".into(),
        })
        .build()
        .unwrap()
}

#[tokio::test]
async fn tm_s3_wave_order_remove_edge_before_set_null_before_cascade() {
    assert_eq!(
        DeletionAction::RemoveEdge {
            edge_table: "e".into()
        }
        .wave_order(),
        0
    );
    assert_eq!(
        DeletionAction::SetNull { field: "fk".into() }.wave_order(),
        1
    );
    assert_eq!(DeletionAction::CascadeDelete.wave_order(), 2);
}

#[tokio::test]
async fn tm_s5_privacy_skips_set_null_nodes() {
    let parent = base_schema("p_s5", vec![]);
    let child = base_schema("c_s5", vec![]);
    let mut reg = SchemaRegistry::new();
    reg.register(parent);
    reg.register(child);

    let v = mem_valence();
    let backend = v.active_backend().unwrap();
    backend
        .create_record(
            "c_s5",
            json!({"id": {"table":"c_s5","id":"c1"}, "parent_id": "p1"}),
        )
        .await
        .unwrap();

    let dag = DeletionDag {
        root_table: "p_s5".into(),
        root_record_id: "p1".into(),
        nodes: vec![
            DeletionNode {
                table: "p_s5".into(),
                record_id: "p1".into(),
                action: DeletionAction::CascadeDelete,
                depth: 0,
                connection_name: "cascade".into(),
                from_table: "p_s5".into(),
            },
            DeletionNode {
                table: "c_s5".into(),
                record_id: "c1".into(),
                action: DeletionAction::SetNull {
                    field: "parent_id".into(),
                },
                depth: 0,
                connection_name: "kids".into(),
                from_table: "p_s5".into(),
            },
        ],
        restrict_violations: vec![],
    };

    // Parent row missing → CascadeDelete skipped; SetNull must not require Delete on child.
    check_dag_delete_privacy_with_registry(&dag, &v, &reg)
        .await
        .expect("SetNull nodes must not fail Delete privacy");
}

#[tokio::test]
async fn tm_s1_apply_set_null_clears_fk_keeps_row() {
    let v = mem_valence();
    let backend = v.active_backend().unwrap();
    backend
        .create_record(
            "child_s1",
            json!({"id": {"table":"child_s1","id":"c1"}, "parent_id": "p1", "name": "keep"}),
        )
        .await
        .unwrap();

    let node = DeletionNode {
        table: "child_s1".into(),
        record_id: "c1".into(),
        action: DeletionAction::SetNull {
            field: "parent_id".into(),
        },
        depth: 0,
        connection_name: "kids".into(),
        from_table: "parent_s1".into(),
    };
    apply_deletion_node(&node, &v).await.expect("set null");

    let row = backend.get_record("child_s1", "c1").await.unwrap().unwrap();
    assert!(row.get("parent_id").unwrap().is_null());
    assert_eq!(row.get("name").and_then(|v| v.as_str()), Some("keep"));
}

#[tokio::test]
async fn tm_s2_apply_remove_edge_clears_edges() {
    let v = mem_valence();
    let from = RecordId::new("proj", "p1");
    let to = RecordId::new("tag", "t1");
    v.relate_edge("proj_tag", &from, &to).await.unwrap();
    assert_eq!(
        v.active_backend()
            .unwrap()
            .get_edge_targets(&from, "proj_tag")
            .await
            .unwrap()
            .len(),
        1
    );

    let node = DeletionNode {
        table: "proj".into(),
        record_id: "p1".into(),
        action: DeletionAction::RemoveEdge {
            edge_table: "proj_tag".into(),
        },
        depth: 0,
        connection_name: "tags".into(),
        from_table: "proj".into(),
    };
    apply_deletion_node(&node, &v).await.expect("remove edge");
    assert!(v
        .active_backend()
        .unwrap()
        .get_edge_targets(&from, "proj_tag")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn tm_s8_idempotent_missing_row_and_empty_edges() {
    let v = mem_valence();
    apply_deletion_node(
        &DeletionNode {
            table: "gone".into(),
            record_id: "x".into(),
            action: DeletionAction::SetNull { field: "fk".into() },
            depth: 0,
            connection_name: "c".into(),
            from_table: "p".into(),
        },
        &v,
    )
    .await
    .expect("missing setnull ok");

    apply_deletion_node(
        &DeletionNode {
            table: "gone".into(),
            record_id: "x".into(),
            action: DeletionAction::RemoveEdge {
                edge_table: "e".into(),
            },
            depth: 0,
            connection_name: "c".into(),
            from_table: "p".into(),
        },
        &v,
    )
    .await
    .expect("empty edges ok");

    apply_deletion_node(
        &DeletionNode {
            table: "gone".into(),
            record_id: "x".into(),
            action: DeletionAction::CascadeDelete,
            depth: 0,
            connection_name: "c".into(),
            from_table: "p".into(),
        },
        &v,
    )
    .await
    .expect("missing cascade ok");
}

#[tokio::test]
async fn tm_s9_hasmany_restrict_blocks_compute() {
    let parent = base_schema(
        "p_s9",
        vec![SchemaConnection {
            name: "kids".into(),
            from_table: "p_s9".into(),
            from_field: "id".into(),
            to_table: "c_s9".into(),
            cardinality: "HasMany".into(),
            required: false,
            on_delete: "Restrict".into(),
            label: "kids".into(),
            model_path: None,
            reverse_field: Some("parent_id".into()),
            edge_table: None,
            target_trait: None,
        }],
    );
    let child = base_schema("c_s9", vec![]);
    let mut reg = SchemaRegistry::new();
    reg.register(parent);
    reg.register(child);

    let v = mem_valence();
    let backend = v.active_backend().unwrap();
    backend
        .create_record(
            "c_s9",
            json!({"id": {"table":"c_s9","id":"c1"}, "parent_id": "p_s9:p1"}),
        )
        .await
        .unwrap();

    let dag = DeletionDag::compute_with_registry(
        "p_s9",
        "p1",
        &v,
        &reg,
        &valence_core::trait_registry::TraitRegistry::new(),
    )
    .await
    .unwrap();
    assert!(!dag.restrict_violations.is_empty());
    assert!(dag.nodes.is_empty());
}
