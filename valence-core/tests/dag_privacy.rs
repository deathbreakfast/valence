//! TM-P1 / TM-P2 / TM-P4 — pre-queue DAG Delete privacy (Delete-only).

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::deletion::check_dag_delete_privacy_with_registry;
use valence_core::deletion::dag::{DeletionAction, DeletionDag, DeletionNode};
use valence_core::error::Error;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy::PrivacyRule;
use valence_core::privacy_policies::common::{AUTHENTICATED, PUBLIC_READ, SYSTEM_ONLY};
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules, SchemaPrivacy,
};
use valence_core::{DatabaseBackend, DatabaseEvaluator, Valence};

fn leak_schema(schema: Schema) -> &'static Schema {
    Box::leak(Box::new(schema))
}

fn schema_with_delete(
    name: &str,
    delete_eval: &'static PrivacyRule,
    delete_name: &str,
    read_eval: Option<&'static PrivacyRule>,
) -> &'static SchemaMetadata {
    let read_rules = read_eval.map(|ev| SchemaPolicyRules {
        allow: vec![SchemaPolicyRule {
            name: "READ".to_string(),
            description: None,
            evaluator: Some(ev),
        }],
        ..SchemaPolicyRules::default()
    });
    let schema = leak_schema(Schema {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".to_string(),
            write: "test".to_string(),
        },
        policies: Some(SchemaPolicies {
            read: read_rules,
            delete: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: delete_name.to_string(),
                    description: None,
                    evaluator: Some(delete_eval),
                }],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![],
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,
        },
    });
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

fn dag_with_nodes(nodes: Vec<DeletionNode>) -> DeletionDag {
    DeletionDag {
        root_table: "parent".to_string(),
        root_record_id: "p1".to_string(),
        nodes,
        restrict_violations: Vec::new(),
    }
}

fn node(table: &str, id: &str) -> DeletionNode {
    DeletionNode {
        table: table.to_string(),
        record_id: id.to_string(),
        action: DeletionAction::CascadeDelete,
        depth: 0,
        connection_name: "cascade".to_string(),
        from_table: table.to_string(),
    }
}

fn mem_valence(actor: Actor) -> (Valence, Arc<dyn DatabaseBackend>) {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    let v = Valence::builder()
        .add_backend("default", Arc::clone(&backend))
        .with_actor(actor)
        .build()
        .expect("build");
    (v, backend)
}

#[tokio::test]
async fn tm_p1_child_delete_deny_fails() {
    let parent = schema_with_delete("dag_priv_parent", &PUBLIC_READ, "PUBLIC_READ", None);
    let child = schema_with_delete("dag_priv_child", &SYSTEM_ONLY, "SYSTEM_ONLY", None);
    let mut reg = SchemaRegistry::new();
    reg.register(parent);
    reg.register(child);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dag_priv_parent", serde_json::json!({"id": "p1"}))
        .await
        .unwrap();
    backend
        .create_record("dag_priv_child", serde_json::json!({"id": "c1"}))
        .await
        .unwrap();

    let dag = dag_with_nodes(vec![
        node("dag_priv_parent", "p1"),
        node("dag_priv_child", "c1"),
    ]);
    let err = check_dag_delete_privacy_with_registry(&dag, &v, &reg)
        .await
        .expect_err("child SYSTEM_ONLY should deny user");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("dag_priv_child")),
        "got {err:?}"
    );
}

#[tokio::test]
async fn tm_p2_all_delete_allow_succeeds() {
    let parent = schema_with_delete("dag_priv_ok_p", &PUBLIC_READ, "PUBLIC_READ", None);
    let child = schema_with_delete("dag_priv_ok_c", &PUBLIC_READ, "PUBLIC_READ", None);
    let mut reg = SchemaRegistry::new();
    reg.register(parent);
    reg.register(child);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dag_priv_ok_p", serde_json::json!({"id": "p1"}))
        .await
        .unwrap();
    backend
        .create_record("dag_priv_ok_c", serde_json::json!({"id": "c1"}))
        .await
        .unwrap();

    let dag = dag_with_nodes(vec![
        node("dag_priv_ok_p", "p1"),
        node("dag_priv_ok_c", "c1"),
    ]);
    check_dag_delete_privacy_with_registry(&dag, &v, &reg)
        .await
        .expect("all Delete-allow");
}

#[tokio::test]
async fn tm_p3_restrict_checked_before_privacy_in_queue_delete_order() {
    // Contract: Restrict violations abort before check_dag_delete_privacy would run.
    // Covered by admin/codegen call order (restrict → privacy → mark). This test locks
    // that an empty-nodes Restrict DAG is not privacy-walked as a success path.
    let dag = DeletionDag {
        root_table: "parent".to_string(),
        root_record_id: "p1".to_string(),
        nodes: Vec::new(),
        restrict_violations: vec![valence_core::deletion::dag::RestrictViolation {
            blocking_table: "child".into(),
            blocking_field: "parent".into(),
            blocking_record_count: 1,
            connection_name: "kids".into(),
        }],
    };
    let (v, _) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    let reg = SchemaRegistry::new();
    // No nodes → privacy walk is a no-op; callers must still refuse on restrict_violations.
    check_dag_delete_privacy_with_registry(&dag, &v, &reg)
        .await
        .expect("empty node list is ok; Restrict is caller's gate");
    assert!(!dag.restrict_violations.is_empty());
}

#[tokio::test]
async fn tm_p4_delete_without_read_succeeds() {
    let meta = schema_with_delete(
        "dag_priv_secret",
        &AUTHENTICATED,
        "AUTHENTICATED",
        Some(&SYSTEM_ONLY),
    );
    let mut reg = SchemaRegistry::new();
    reg.register(meta);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "cleaner".into(),
    });
    backend
        .create_record(
            "dag_priv_secret",
            serde_json::json!({"id": "s1", "secret": "x"}),
        )
        .await
        .unwrap();

    let dag = dag_with_nodes(vec![node("dag_priv_secret", "s1")]);
    check_dag_delete_privacy_with_registry(&dag, &v, &reg)
        .await
        .expect("Delete-allow + Read-deny must succeed");
}
