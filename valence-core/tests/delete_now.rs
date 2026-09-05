//! TM-1..TM-7 — synchronous `delete_entity_now` / prepare path (in-memory).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::deletion::dag::{DeletionAction, DeletionDag};
use valence_core::deletion::{
    delete_entity_now, normalize_record_id_for_deletion, prepare_deletion, DeletionMode,
    PreparedDeletion,
};
use valence_core::error::Error;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::owner_ref::OwnerRef;
use valence_core::ownership::OwnershipService;
use valence_core::privacy::PrivacyRule;
use valence_core::privacy_policies::common::{AUTHENTICATED, PUBLIC_READ, SYSTEM_ONLY};
use valence_core::query::QueryCore;
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

fn public_delete_schema(name: &str, connections: Vec<SchemaConnection>) -> &'static SchemaMetadata {
    schema_with_policies(name, connections, &PUBLIC_READ, "PUBLIC", None)
}

fn schema_with_policies(
    name: &str,
    connections: Vec<SchemaConnection>,
    delete_eval: &'static PrivacyRule,
    delete_name: &str,
    read_eval: Option<&'static PrivacyRule>,
) -> &'static SchemaMetadata {
    let read_rules = read_eval.map(|ev| SchemaPolicyRules {
        allow: vec![SchemaPolicyRule {
            name: "READ".into(),
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
            read: "t".into(),
            write: "t".into(),
        },
        policies: Some(SchemaPolicies {
            read: read_rules,
            delete: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: delete_name.into(),
                    description: None,
                    evaluator: Some(delete_eval),
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

fn has_many(name: &str, from: &str, to: &str, reverse: &str, on_delete: &str) -> SchemaConnection {
    SchemaConnection {
        name: name.into(),
        from_table: from.into(),
        from_field: "id".into(),
        to_table: to.into(),
        cardinality: "HasMany".into(),
        required: false,
        on_delete: on_delete.into(),
        label: name.into(),
        model_path: None,
        reverse_field: Some(reverse.into()),
        edge_table: None,
        target_trait: None,
    }
}

fn m2m(name: &str, from: &str, to: &str, edge: &str, on_delete: &str) -> SchemaConnection {
    SchemaConnection {
        name: name.into(),
        from_table: from.into(),
        from_field: "id".into(),
        to_table: to.into(),
        cardinality: "ManyToMany".into(),
        required: false,
        on_delete: on_delete.into(),
        label: name.into(),
        model_path: None,
        reverse_field: None,
        edge_table: Some(edge.into()),
        target_trait: None,
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

fn ensure_dn_schemas() {
    let reg = SchemaRegistry::global();
    for table in [
        "dn_root",
        "dn_child",
        "dn_ref",
        "dn_peer",
        "dn_priv_p",
        "dn_priv_c",
        "dn_secret",
        "dn_restrict_p",
        "dn_restrict_c",
        "dn_id",
        "dn_pgp",
        "dn_pending",
        "dn_missing",
    ] {
        assert!(
            reg.get_schema(table).is_some(),
            "delete_now schema {table} missing from SchemaRegistry::global (inventory link?)"
        );
    }
}

// --- Inventory schemas (global registry for prepare_deletion / delete_entity_now) ---

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        public_delete_schema(
            "dn_root",
            vec![
                has_many("kids", "dn_root", "dn_child", "parent_id", "Cascade"),
                has_many("refs", "dn_root", "dn_ref", "parent_id", "SetNull"),
                m2m("tags", "dn_root", "dn_peer", "dn_root_peer", "SetNull"),
            ],
        )
    })
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_child", vec![]))
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_ref", vec![]))
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_peer", vec![]))
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        public_delete_schema(
            "dn_priv_p",
            vec![has_many(
                "kids",
                "dn_priv_p",
                "dn_priv_c",
                "parent_id",
                "Cascade",
            )],
        )
    })
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        schema_with_policies("dn_priv_c", vec![], &SYSTEM_ONLY, "SYSTEM_ONLY", None)
    })
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        schema_with_policies(
            "dn_secret",
            vec![],
            &AUTHENTICATED,
            "AUTHENTICATED",
            Some(&SYSTEM_ONLY),
        )
    })
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        public_delete_schema(
            "dn_restrict_p",
            vec![has_many(
                "kids",
                "dn_restrict_p",
                "dn_restrict_c",
                "parent_id",
                "Restrict",
            )],
        )
    })
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_restrict_c", vec![]))
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_id", vec![]))
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_pgp", vec![]))
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_pending", vec![]))
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| public_delete_schema("dn_missing", vec![]))
}

#[tokio::test]
async fn delete_now_cascades_children_and_removes_root() {
    ensure_dn_schemas();
    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dn_root", json!({"id": "p1", "name": "root"}))
        .await
        .unwrap();
    backend
        .create_record("dn_child", json!({"id": "c1", "parent_id": "dn_root:p1"}))
        .await
        .unwrap();
    backend
        .create_record(
            "dn_ref",
            json!({"id": "r1", "parent_id": "dn_root:p1", "name": "keep"}),
        )
        .await
        .unwrap();
    backend
        .create_record("dn_peer", json!({"id": "t1"}))
        .await
        .unwrap();
    let from = RecordId::new("dn_root", "p1");
    let to = RecordId::new("dn_peer", "t1");
    v.relate_edge("dn_root_peer", &from, &to).await.unwrap();

    delete_entity_now("dn_root", "p1", &v)
        .await
        .expect("cascade delete_now");

    assert!(QueryCore::get_record_json("dn_root", "p1", &v)
        .await
        .unwrap()
        .is_none());
    assert!(QueryCore::get_record_json("dn_child", "c1", &v)
        .await
        .unwrap()
        .is_none());
    let referrer = QueryCore::get_record_json("dn_ref", "r1", &v)
        .await
        .unwrap()
        .expect("SetNull row remains");
    assert!(referrer.get("parent_id").unwrap().is_null());
    assert_eq!(referrer.get("name").and_then(|x| x.as_str()), Some("keep"));
    assert!(backend
        .get_edge_targets(&from, "dn_root_peer")
        .await
        .unwrap()
        .is_empty());
    assert!(QueryCore::get_record_json("dn_peer", "t1", &v)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_now_child_privacy_denial_changes_nothing() {
    ensure_dn_schemas();
    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dn_priv_p", json!({"id": "p1"}))
        .await
        .unwrap();
    backend
        .create_record(
            "dn_priv_c",
            json!({"id": "c1", "parent_id": "dn_priv_p:p1"}),
        )
        .await
        .unwrap();

    let err = delete_entity_now("dn_priv_p", "p1", &v)
        .await
        .expect_err("child SYSTEM_ONLY delete must deny");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("dn_priv_c")),
        "got {err:?}"
    );

    assert!(QueryCore::get_record_json("dn_priv_p", "p1", &v)
        .await
        .unwrap()
        .is_some());
    assert!(QueryCore::get_record_json("dn_priv_c", "c1", &v)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_now_does_not_require_read_when_delete_allows() {
    ensure_dn_schemas();
    let (v, backend) = mem_valence(Actor::User {
        user_id: "cleaner".into(),
    });
    backend
        .create_record("dn_secret", json!({"id": "s1", "secret": "x"}))
        .await
        .unwrap();

    delete_entity_now("dn_secret", "s1", &v)
        .await
        .expect("Delete-allow + Read-deny");
    assert!(QueryCore::get_record_json("dn_secret", "s1", &v)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn delete_now_restrict_violation_changes_nothing() {
    ensure_dn_schemas();
    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dn_restrict_p", json!({"id": "p1"}))
        .await
        .unwrap();
    backend
        .create_record(
            "dn_restrict_c",
            json!({"id": "c1", "parent_id": "dn_restrict_p:p1"}),
        )
        .await
        .unwrap();

    let err = delete_entity_now("dn_restrict_p", "p1", &v)
        .await
        .expect_err("Restrict must block");
    assert!(
        matches!(err, Error::Validation(ref m) if m.to_lowercase().contains("restrict")),
        "got {err:?}"
    );

    assert!(QueryCore::get_record_json("dn_restrict_p", "p1", &v)
        .await
        .unwrap()
        .is_some());
    assert!(QueryCore::get_record_json("dn_restrict_c", "c1", &v)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_now_uses_dag_execution_order() {
    ensure_dn_schemas();
    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dn_root", json!({"id": "ord1"}))
        .await
        .unwrap();
    backend
        .create_record(
            "dn_child",
            json!({"id": "oc1", "parent_id": "dn_root:ord1"}),
        )
        .await
        .unwrap();
    backend
        .create_record("dn_ref", json!({"id": "or1", "parent_id": "dn_root:ord1"}))
        .await
        .unwrap();
    backend
        .create_record("dn_peer", json!({"id": "ot1"}))
        .await
        .unwrap();
    let from = RecordId::new("dn_root", "ord1");
    let to = RecordId::new("dn_peer", "ot1");
    v.relate_edge("dn_root_peer", &from, &to).await.unwrap();

    let PreparedDeletion::Ready { dag, .. } =
        prepare_deletion("dn_root", "ord1", DeletionMode::Now, &v)
            .await
            .expect("prepare")
    else {
        panic!("expected Ready DAG");
    };

    let mut expected = dag.nodes.clone();
    DeletionDag::sort_for_execution(&mut expected);
    assert_eq!(
        dag.nodes.len(),
        expected.len(),
        "prepared DAG should already be sorted for execution"
    );
    for (got, want) in dag.nodes.iter().zip(expected.iter()) {
        assert_eq!(got.table, want.table);
        assert_eq!(got.record_id, want.record_id);
        assert_eq!(got.action, want.action);
        assert_eq!(got.depth, want.depth);
    }

    // Within depth 0 of the root: RemoveEdge → SetNull → CascadeDelete (root last among cascades).
    let depth0: Vec<_> = dag.nodes.iter().filter(|n| n.depth == 0).collect();
    let mut prev = 0u8;
    for w in depth0.iter().map(|n| n.action.wave_order()) {
        assert!(w >= prev, "wave order must be non-decreasing within depth");
        prev = w;
    }
    assert!(
        depth0
            .iter()
            .any(|n| matches!(n.action, DeletionAction::RemoveEdge { .. })),
        "expected RemoveEdge at depth 0"
    );
    assert!(
        depth0
            .iter()
            .any(|n| matches!(n.action, DeletionAction::SetNull { .. })),
        "expected SetNull at depth 0"
    );
    // Cascade child is deeper than root and must appear earlier in execution order.
    let child = dag
        .nodes
        .iter()
        .find(|n| n.table == "dn_child" && matches!(n.action, DeletionAction::CascadeDelete))
        .expect("cascade child node");
    let root = dag
        .nodes
        .iter()
        .find(|n| n.table == "dn_root" && matches!(n.action, DeletionAction::CascadeDelete))
        .expect("cascade root node");
    assert!(
        child.depth > root.depth,
        "child cascade depth {} must exceed root depth {}",
        child.depth,
        root.depth
    );
    let ci = dag
        .nodes
        .iter()
        .position(|n| std::ptr::eq(n, child))
        .unwrap();
    let ri = dag
        .nodes
        .iter()
        .position(|n| std::ptr::eq(n, root))
        .unwrap();
    assert!(
        ci < ri,
        "child CascadeDelete before root in execution order"
    );
}

#[tokio::test]
async fn delete_now_accepts_bare_and_matching_qualified_ids() {
    ensure_dn_schemas();
    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dn_id", json!({"id": "42"}))
        .await
        .unwrap();
    assert_eq!(normalize_record_id_for_deletion("dn_id", "dn_id:42"), "42");
    delete_entity_now("dn_id", "dn_id:42", &v)
        .await
        .expect("qualified id");
    assert!(QueryCore::get_record_json("dn_id", "42", &v)
        .await
        .unwrap()
        .is_none());

    backend
        .create_record("dn_id", json!({"id": "bare"}))
        .await
        .unwrap();
    delete_entity_now("dn_id", "bare", &v)
        .await
        .expect("bare id");
    assert!(QueryCore::get_record_json("dn_id", "bare", &v)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn delete_now_preserves_colon_bearing_literal_primary_key() {
    ensure_dn_schemas();
    let literal = "permission_group:owners_xyz";
    assert_eq!(
        normalize_record_id_for_deletion("dn_pgp", literal),
        literal,
        "foreign table prefix must not be stripped"
    );

    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dn_pgp", json!({"id": literal, "role": "owner"}))
        .await
        .unwrap();

    delete_entity_now("dn_pgp", literal, &v)
        .await
        .expect("colon-bearing PK");
    assert!(QueryCore::get_record_json("dn_pgp", literal, &v)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn delete_now_rejects_pending_queued_root() {
    ensure_dn_schemas();
    let (v, backend) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    backend
        .create_record("dn_pending", json!({"id": "q1"}))
        .await
        .unwrap();
    OwnershipService::ensure_active_ownership("dn_pending", "q1", OwnerRef::system(), &v)
        .await
        .unwrap();
    OwnershipService::mark_pending_deletion("dn_pending", "q1", &v)
        .await
        .unwrap();

    let err = delete_entity_now("dn_pending", "q1", &v)
        .await
        .expect_err("pending root refused");
    assert!(matches!(err, Error::PendingDeletion(_)), "got {err:?}");
    assert!(QueryCore::get_record_json("dn_pending", "q1", &v)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_now_missing_row_returns_ok() {
    ensure_dn_schemas();
    let (v, _) = mem_valence(Actor::User {
        user_id: "u1".into(),
    });
    delete_entity_now("dn_missing", "no-such-row", &v)
        .await
        .expect("missing is Ok");
}
