//! Defer-to-edge read privacy integration (RH-01).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::error::Error;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy::{PrivacyEvaluator, DEFER_TO_EDGE_MAX_DEPTH};
use valence_core::privacy_policies::common::{AUTHENTICATED, PUBLIC_READ, SYSTEM_ONLY};
use valence_core::privacy_policies::owner::OWNER_BY_USER_FIELD;
use valence_core::query::QueryCore;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    ForeignKeyRef, Schema, SchemaConnection, SchemaField, SchemaMeta, SchemaPolicies,
    SchemaPolicyRule, SchemaPolicyRules, SchemaPrivacy,
};
use valence_core::{DatabaseBackend, DatabaseEvaluator, Valence};

fn leak_schema(schema: Schema) -> &'static Schema {
    Box::leak(Box::new(schema))
}

fn base_schema(name: &str, policies: SchemaPolicies, fields: Vec<SchemaField>) -> &'static Schema {
    leak_schema(Schema {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".to_string(),
            write: "test".to_string(),
        },
        policies: Some(policies),
        fields,
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
    })
}

fn meta(schema: &'static Schema) -> &'static SchemaMetadata {
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

fn id_field() -> SchemaField {
    SchemaField {
        name: "id".to_string(),
        field_type: "string".to_string(),
        primary: true,
        nullable: false,
        indexed: false,
        unique: false,
        default: None,
        fk: None,
        validations: Vec::new(),
        policies: None,
        encrypted: false,
        enum_variants: Vec::new(),
        enum_type: None,
        model_path: None,
    }
}

fn user_field() -> SchemaField {
    SchemaField {
        name: "user".to_string(),
        field_type: "string".to_string(),
        primary: false,
        nullable: false,
        indexed: false,
        unique: false,
        default: None,
        fk: None,
        validations: Vec::new(),
        policies: None,
        encrypted: false,
        enum_variants: Vec::new(),
        enum_type: None,
        model_path: None,
    }
}

fn source_field(parent_table: &str) -> SchemaField {
    SchemaField {
        name: "source".to_string(),
        field_type: format!("Record({parent_table})"),
        primary: false,
        nullable: false,
        indexed: false,
        unique: false,
        default: None,
        fk: Some(ForeignKeyRef {
            ref_table: parent_table.to_string(),
            field: "id".to_string(),
        }),
        validations: Vec::new(),
        policies: None,
        encrypted: false,
        enum_variants: Vec::new(),
        enum_type: None,
        model_path: None,
    }
}

fn allow_rule(name: &str, eval: &'static valence_core::privacy::PrivacyRule) -> SchemaPolicyRule {
    SchemaPolicyRule {
        name: name.to_string(),
        description: None,
        evaluator: Some(eval),
    }
}

fn parent_owner_schema(table: &str) -> &'static SchemaMetadata {
    meta(base_schema(
        table,
        SchemaPolicies {
            read: Some(SchemaPolicyRules {
                allow: vec![allow_rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
            create: Some(SchemaPolicyRules {
                allow: vec![allow_rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            update: Some(SchemaPolicyRules {
                allow: vec![allow_rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
            delete: Some(SchemaPolicyRules {
                allow: vec![allow_rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
        },
        vec![id_field(), user_field()],
    ))
}

fn history_defer_schema(table: &str, parent_table: &str) -> &'static SchemaMetadata {
    meta(leak_schema(Schema {
        name: table.to_string(),
        version: "0.1.0".to_string(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".to_string(),
            write: "test".to_string(),
        },
        policies: Some(SchemaPolicies {
            read: Some(SchemaPolicyRules {
                always_allow: vec![allow_rule("SYSTEM", &SYSTEM_ONLY)],
                defer_to_edge: Some("source".to_string()),
                ..SchemaPolicyRules::default()
            }),
            create: Some(SchemaPolicyRules {
                defer_to_edge: Some("source".to_string()),
                ..SchemaPolicyRules::default()
            }),
            update: Some(SchemaPolicyRules {
                defer_to_edge: Some("source".to_string()),
                ..SchemaPolicyRules::default()
            }),
            delete: Some(SchemaPolicyRules {
                defer_to_edge: Some("source".to_string()),
                ..SchemaPolicyRules::default()
            }),
        }),
        fields: vec![id_field(), source_field(parent_table)],
        edges: Vec::new(),
        connections: vec![SchemaConnection {
            name: "source".to_string(),
            from_table: table.to_string(),
            from_field: "source".to_string(),
            to_table: parent_table.to_string(),
            cardinality: "HasOne".to_string(),
            required: true,
            on_delete: "Cascade".to_string(),
            label: "source".to_string(),
            model_path: None,
            reverse_field: None,
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
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,
        },
    }))
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

fn register_pair(parent: &'static SchemaMetadata, hist: &'static SchemaMetadata) {
    SchemaRegistry::register_overlay(parent);
    SchemaRegistry::register_overlay(hist);
}

#[tokio::test]
async fn defer_allows_when_parent_readable_happy() {
    let parent = parent_owner_schema("defer_parent_ok");
    let hist = history_defer_schema("defer_hist_ok", "defer_parent_ok");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_parent_ok",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_hist_ok",
            serde_json::json!({
                "id": "h1",
                "source": {"table": "defer_parent_ok", "id": "p1"}
            }),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_hist_ok", "h1", &v)
        .await
        .unwrap()
        .expect("hist row");
    PrivacyEvaluator::check_entity_read(hist, &raw, &v)
        .await
        .expect("owner may read history via defer");
}

#[tokio::test]
async fn defer_denies_when_parent_unreadable_sad() {
    let parent = parent_owner_schema("defer_parent_deny");
    let hist = history_defer_schema("defer_hist_deny", "defer_parent_deny");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "bob".into(),
    });
    backend
        .create_record(
            "defer_parent_deny",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_hist_deny",
            serde_json::json!({
                "id": "h1",
                "source": {"table": "defer_parent_deny", "id": "p1"}
            }),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_hist_deny", "h1", &v)
        .await
        .unwrap()
        .expect("hist row");
    let err = PrivacyEvaluator::check_entity_read(hist, &raw, &v)
        .await
        .expect_err("stranger must not read via defer");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
}

#[tokio::test]
async fn defer_missing_source_denies_sad() {
    let parent = parent_owner_schema("defer_parent_nosrc");
    let hist = history_defer_schema("defer_hist_nosrc", "defer_parent_nosrc");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_hist_nosrc",
            serde_json::json!({"id": "h1", "source": serde_json::Value::Null}),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_hist_nosrc", "h1", &v)
        .await
        .unwrap()
        .expect("hist row");
    let err = PrivacyEvaluator::check_entity_read(hist, &raw, &v)
        .await
        .expect_err("null source denies");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("missing")),
        "{err:?}"
    );
}

#[tokio::test]
async fn defer_missing_parent_denies_sad() {
    let parent = parent_owner_schema("defer_parent_gone");
    let hist = history_defer_schema("defer_hist_gone", "defer_parent_gone");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_hist_gone",
            serde_json::json!({
                "id": "h1",
                "source": {"table": "defer_parent_gone", "id": "missing"}
            }),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_hist_gone", "h1", &v)
        .await
        .unwrap()
        .expect("hist row");
    let err = PrivacyEvaluator::check_entity_read(hist, &raw, &v)
        .await
        .expect_err("missing parent denies");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("not found")),
        "{err:?}"
    );
}

#[tokio::test]
async fn defer_always_allow_super_user_happy() {
    // SYSTEM_ONLY on always_allow of history — Actor::System matches via SYSTEM_ONLY rule.
    let parent = parent_owner_schema("defer_parent_sys");
    let hist = history_defer_schema("defer_hist_sys", "defer_parent_sys");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::System {
        operation: "test".into(),
    });
    backend
        .create_record(
            "defer_hist_sys",
            serde_json::json!({
                "id": "h1",
                "source": {"table": "defer_parent_sys", "id": "nope"}
            }),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_hist_sys", "h1", &v)
        .await
        .unwrap()
        .expect("hist row");
    PrivacyEvaluator::check_entity_read(hist, &raw, &v)
        .await
        .expect("System always_allow bypasses defer");
}

#[tokio::test]
async fn defer_filters_list_sad_and_happy() {
    let parent = parent_owner_schema("defer_parent_list");
    let hist = history_defer_schema("defer_hist_list", "defer_parent_list");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_parent_list",
            serde_json::json!({"id": "pa", "user": "alice"}),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_parent_list",
            serde_json::json!({"id": "pb", "user": "bob"}),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_hist_list",
            serde_json::json!({
                "id": "ha",
                "source": {"table": "defer_parent_list", "id": "pa"}
            }),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_hist_list",
            serde_json::json!({
                "id": "hb",
                "source": {"table": "defer_parent_list", "id": "pb"}
            }),
        )
        .await
        .unwrap();

    for (id, expect_ok) in [("ha", true), ("hb", false)] {
        let raw = QueryCore::get_record_json("defer_hist_list", id, &v)
            .await
            .unwrap()
            .expect("row");
        let res = PrivacyEvaluator::check_entity_read(hist, &raw, &v).await;
        assert_eq!(res.is_ok(), expect_ok, "id={id} err={res:?}");
    }
}

#[tokio::test]
async fn defer_cycle_denies_sad() {
    // A defers to B, B defers to A.
    let a = history_defer_schema("defer_cycle_a", "defer_cycle_b");
    let b = history_defer_schema("defer_cycle_b", "defer_cycle_a");
    // Replace always_allow SYSTEM so we actually take the defer path as a user.
    let a_schema = leak_schema(Schema {
        name: "defer_cycle_a".into(),
        version: "0.1.0".into(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".into(),
            write: "test".into(),
        },
        policies: Some(SchemaPolicies {
            read: Some(SchemaPolicyRules {
                defer_to_edge: Some("source".into()),
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![id_field(), source_field("defer_cycle_b")],
        edges: vec![],
        connections: vec![SchemaConnection {
            name: "source".into(),
            from_table: "defer_cycle_a".into(),
            from_field: "source".into(),
            to_table: "defer_cycle_b".into(),
            cardinality: "HasOne".into(),
            required: true,
            on_delete: "Cascade".into(),
            label: "source".into(),
            model_path: None,
            reverse_field: None,
            edge_table: None,
            target_trait: None,
        }],
        side_effects: vec![],
        iters: vec![],
        composite_key: vec![],
        traits: vec![],
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".into(),
            row_count: 0,
            owner: "system".into(),
            description: None,
        },
    });
    let b_schema = leak_schema(Schema {
        name: "defer_cycle_b".into(),
        version: "0.1.0".into(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".into(),
            write: "test".into(),
        },
        policies: Some(SchemaPolicies {
            read: Some(SchemaPolicyRules {
                defer_to_edge: Some("source".into()),
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![id_field(), source_field("defer_cycle_a")],
        edges: vec![],
        connections: vec![SchemaConnection {
            name: "source".into(),
            from_table: "defer_cycle_b".into(),
            from_field: "source".into(),
            to_table: "defer_cycle_a".into(),
            cardinality: "HasOne".into(),
            required: true,
            on_delete: "Cascade".into(),
            label: "source".into(),
            model_path: None,
            reverse_field: None,
            edge_table: None,
            target_trait: None,
        }],
        side_effects: vec![],
        iters: vec![],
        composite_key: vec![],
        traits: vec![],
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".into(),
            row_count: 0,
            owner: "system".into(),
            description: None,
        },
    });
    let a_meta = meta(a_schema);
    let b_meta = meta(b_schema);
    register_pair(a_meta, b_meta);
    let _ = (a, b);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_cycle_a",
            serde_json::json!({
                "id": "a1",
                "source": {"table": "defer_cycle_b", "id": "b1"}
            }),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_cycle_b",
            serde_json::json!({
                "id": "b1",
                "source": {"table": "defer_cycle_a", "id": "a1"}
            }),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_cycle_a", "a1", &v)
        .await
        .unwrap()
        .unwrap();
    let err = PrivacyEvaluator::check_entity_read(a_meta, &raw, &v)
        .await
        .expect_err("cycle must deny");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("cycle")),
        "{err:?}"
    );
}

#[tokio::test]
async fn create_defers_to_parent_update_allows_when_parent_updatable_happy() {
    let parent = parent_owner_schema("defer_parent_create_ok");
    let hist = history_defer_schema("defer_hist_create_ok", "defer_parent_create_ok");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_parent_create_ok",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();

    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_create_ok", "id": "p1"}
    });
    PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Create,
        &raw,
        &v,
    )
    .await
    .expect("owner create defers to parent Update");
}

#[tokio::test]
async fn create_defers_to_parent_update_denies_when_parent_update_blocked_sad() {
    let parent = parent_owner_schema("defer_parent_create_deny");
    let hist = history_defer_schema("defer_hist_create_deny", "defer_parent_create_deny");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "bob".into(),
    });
    backend
        .create_record(
            "defer_parent_create_deny",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();

    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_create_deny", "id": "p1"}
    });
    let err = PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Create,
        &raw,
        &v,
    )
    .await
    .expect_err("stranger create must deny");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
}

#[tokio::test]
async fn create_defer_does_not_use_parent_read_policy_sad() {
    // Parent: AUTHENTICATED Read, OWNER Update — peer can Read but not Update.
    let parent = meta(base_schema(
        "defer_parent_read_not_update",
        SchemaPolicies {
            read: Some(SchemaPolicyRules {
                allow: vec![allow_rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            create: Some(SchemaPolicyRules {
                allow: vec![allow_rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            update: Some(SchemaPolicyRules {
                allow: vec![allow_rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
            delete: Some(SchemaPolicyRules {
                allow: vec![allow_rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
        },
        vec![id_field(), user_field()],
    ));
    let hist = history_defer_schema("defer_hist_read_not_update", "defer_parent_read_not_update");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "bob".into(),
    });
    backend
        .create_record(
            "defer_parent_read_not_update",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();

    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_read_not_update", "id": "p1"}
    });
    PrivacyEvaluator::check_entity_read(hist, &raw, &v)
        .await
        .expect("peer may read via parent Read");
    let err = PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Create,
        &raw,
        &v,
    )
    .await
    .expect_err("peer must not create via parent Read alone");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
}

#[tokio::test]
async fn create_defer_missing_source_denies_sad() {
    let parent = parent_owner_schema("defer_parent_create_nosrc");
    let hist = history_defer_schema("defer_hist_create_nosrc", "defer_parent_create_nosrc");
    register_pair(parent, hist);
    let (v, _) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    let raw = serde_json::json!({"id": "h1", "source": serde_json::Value::Null});
    let err = PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Create,
        &raw,
        &v,
    )
    .await
    .expect_err("null source");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("missing")),
        "{err:?}"
    );
}

#[tokio::test]
async fn create_defer_missing_parent_denies_sad() {
    let parent = parent_owner_schema("defer_parent_create_gone");
    let hist = history_defer_schema("defer_hist_create_gone", "defer_parent_create_gone");
    register_pair(parent, hist);
    let (v, _) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_create_gone", "id": "missing"}
    });
    let err = PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Create,
        &raw,
        &v,
    )
    .await
    .expect_err("missing parent");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("not found")),
        "{err:?}"
    );
}

#[tokio::test]
async fn create_defer_system_parent_fetch_does_not_elevate_viewer_happy() {
    let parent = parent_owner_schema("defer_parent_create_viewer");
    let hist = history_defer_schema("defer_hist_create_viewer", "defer_parent_create_viewer");
    register_pair(parent, hist);
    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_parent_create_viewer",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_create_viewer", "id": "p1"}
    });
    PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Create,
        &raw,
        &v,
    )
    .await
    .expect("create ok");
    assert!(
        matches!(v.actor(), Actor::User { user_id } if user_id == "alice"),
        "viewer must remain User after create defer"
    );
}

#[tokio::test]
async fn update_defers_to_parent_update_allows_happy() {
    let parent = parent_owner_schema("defer_parent_upd_ok");
    let hist = history_defer_schema("defer_hist_upd_ok", "defer_parent_upd_ok");
    register_pair(parent, hist);
    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_parent_upd_ok",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_upd_ok", "id": "p1"}
    });
    PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Update,
        &raw,
        &v,
    )
    .await
    .expect("owner update defers");
}

#[tokio::test]
async fn update_defers_to_parent_update_denies_sad() {
    let parent = parent_owner_schema("defer_parent_upd_deny");
    let hist = history_defer_schema("defer_hist_upd_deny", "defer_parent_upd_deny");
    register_pair(parent, hist);
    let (v, backend) = mem_valence(Actor::User {
        user_id: "bob".into(),
    });
    backend
        .create_record(
            "defer_parent_upd_deny",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_upd_deny", "id": "p1"}
    });
    let err = PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Update,
        &raw,
        &v,
    )
    .await
    .expect_err("stranger update denied");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
}

#[tokio::test]
async fn delete_defers_to_parent_delete_allows_happy() {
    let parent = parent_owner_schema("defer_parent_del_ok");
    let hist = history_defer_schema("defer_hist_del_ok", "defer_parent_del_ok");
    register_pair(parent, hist);
    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_parent_del_ok",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_del_ok", "id": "p1"}
    });
    PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Delete,
        &raw,
        &v,
    )
    .await
    .expect("owner delete defers");
}

#[tokio::test]
async fn delete_defers_to_parent_delete_denies_sad() {
    let parent = parent_owner_schema("defer_parent_del_deny");
    let hist = history_defer_schema("defer_hist_del_deny", "defer_parent_del_deny");
    register_pair(parent, hist);
    let (v, backend) = mem_valence(Actor::User {
        user_id: "bob".into(),
    });
    backend
        .create_record(
            "defer_parent_del_deny",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_del_deny", "id": "p1"}
    });
    let err = PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Delete,
        &raw,
        &v,
    )
    .await
    .expect_err("stranger delete denied");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
}

#[tokio::test]
async fn delete_defer_does_not_use_parent_read_or_update_sad() {
    // Parent: AUTHENTICATED Read+Update, OWNER Delete — peer can update parent but not delete.
    let parent = meta(base_schema(
        "defer_parent_del_not_upd",
        SchemaPolicies {
            read: Some(SchemaPolicyRules {
                allow: vec![allow_rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            create: Some(SchemaPolicyRules {
                allow: vec![allow_rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            update: Some(SchemaPolicyRules {
                allow: vec![allow_rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            delete: Some(SchemaPolicyRules {
                allow: vec![allow_rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
        },
        vec![id_field(), user_field()],
    ));
    let hist = history_defer_schema("defer_hist_del_not_upd", "defer_parent_del_not_upd");
    register_pair(parent, hist);
    let (v, backend) = mem_valence(Actor::User {
        user_id: "bob".into(),
    });
    backend
        .create_record(
            "defer_parent_del_not_upd",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    let raw = serde_json::json!({
        "id": "h1",
        "source": {"table": "defer_parent_del_not_upd", "id": "p1"}
    });
    PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Update,
        &raw,
        &v,
    )
    .await
    .expect("peer may update history via parent Update");
    let err = PrivacyEvaluator::check_entity_access(
        hist,
        valence_core::privacy::PrivacyOperation::Delete,
        &raw,
        &v,
    )
    .await
    .expect_err("peer must not delete via Read/Update alone");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
}

#[test]
fn parent_op_for_defer_create_maps_to_update_unit() {
    use valence_core::privacy::{parent_op_for_defer, PrivacyOperation};
    assert_eq!(
        parent_op_for_defer(PrivacyOperation::Create),
        PrivacyOperation::Update
    );
    assert_eq!(
        parent_op_for_defer(PrivacyOperation::Read),
        PrivacyOperation::Read
    );
    assert_eq!(
        parent_op_for_defer(PrivacyOperation::Delete),
        PrivacyOperation::Delete
    );
}

#[tokio::test]
async fn validate_unknown_edge_errors() {
    let schema = leak_schema(Schema {
        name: "defer_bad_edge".into(),
        version: "0.1.0".into(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".into(),
            write: "test".into(),
        },
        policies: Some(SchemaPolicies {
            read: Some(SchemaPolicyRules {
                defer_to_edge: Some("nope".into()),
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![id_field()],
        edges: vec![],
        connections: vec![],
        side_effects: vec![],
        iters: vec![],
        composite_key: vec![],
        traits: vec![],
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".into(),
            row_count: 0,
            owner: "system".into(),
            description: None,
        },
    });
    let m = meta(schema);
    SchemaRegistry::register_overlay(m);
    let (v, _) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    let err = PrivacyEvaluator::check_entity_read(m, &serde_json::json!({"id": "1"}), &v)
        .await
        .expect_err("bad edge");
    assert!(
        matches!(err, Error::Validation(ref msg) if msg.contains("defer_to_edge")),
        "{err:?}"
    );
}

#[tokio::test]
async fn defer_depth_exceeded_denies_sad() {
    // Build a chain longer than DEFER_TO_EDGE_MAX_DEPTH (8). Each hop defers to the next.
    let depth = DEFER_TO_EDGE_MAX_DEPTH as usize;
    let mut metas = Vec::new();
    for i in 0..=depth {
        let table = format!("defer_depth_{i}");
        let parent = format!("defer_depth_{}", i + 1);
        let hist = history_defer_schema(&table, &parent);
        SchemaRegistry::register_overlay(hist);
        metas.push(hist);
    }
    // Terminal parent row (never reached — depth guard trips first).
    let terminal = parent_owner_schema(&format!("defer_depth_{}", depth + 1));
    SchemaRegistry::register_overlay(terminal);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    for i in 0..=depth {
        let table = format!("defer_depth_{i}");
        let parent = format!("defer_depth_{}", i + 1);
        backend
            .create_record(
                &table,
                serde_json::json!({
                    "id": format!("n{i}"),
                    "source": {"table": parent, "id": format!("n{}", i + 1)}
                }),
            )
            .await
            .unwrap();
    }
    backend
        .create_record(
            &format!("defer_depth_{}", depth + 1),
            serde_json::json!({"id": format!("n{}", depth + 1), "user": "alice"}),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_depth_0", "n0", &v)
        .await
        .unwrap()
        .expect("row");
    let err = PrivacyEvaluator::check_entity_read(metas[0], &raw, &v)
        .await
        .expect_err("depth exceeded must deny");
    assert!(
        matches!(err, Error::Privacy(ref m) if m.contains("depth")),
        "{err:?}"
    );
}

#[tokio::test]
async fn defer_nested_chain_owner_allows_stranger_denies() {
    // hist -> mid -> parent(owner). Two defer hops.
    let parent = parent_owner_schema("defer_nest_parent");
    let mid = history_defer_schema("defer_nest_mid", "defer_nest_parent");
    let hist = history_defer_schema("defer_nest_hist", "defer_nest_mid");
    SchemaRegistry::register_overlay(parent);
    SchemaRegistry::register_overlay(mid);
    SchemaRegistry::register_overlay(hist);

    let (owner_v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_nest_parent",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_nest_mid",
            serde_json::json!({
                "id": "m1",
                "source": {"table": "defer_nest_parent", "id": "p1"}
            }),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_nest_hist",
            serde_json::json!({
                "id": "h1",
                "source": {"table": "defer_nest_mid", "id": "m1"}
            }),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_nest_hist", "h1", &owner_v)
        .await
        .unwrap()
        .expect("hist");
    PrivacyEvaluator::check_entity_read(hist, &raw, &owner_v)
        .await
        .expect("owner may read via nested defer");

    let stranger = owner_v.with_actor(Actor::User {
        user_id: "bob".into(),
    });
    let err = PrivacyEvaluator::check_entity_read(hist, &raw, &stranger)
        .await
        .expect_err("stranger denied on nested defer");
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
}

#[tokio::test]
async fn defer_system_parent_fetch_does_not_elevate_viewer() {
    let parent = parent_owner_schema("defer_sys_parent");
    let hist = history_defer_schema("defer_sys_hist", "defer_sys_parent");
    register_pair(parent, hist);

    let (v, backend) = mem_valence(Actor::User {
        user_id: "alice".into(),
    });
    backend
        .create_record(
            "defer_sys_parent",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await
        .unwrap();
    backend
        .create_record(
            "defer_sys_hist",
            serde_json::json!({
                "id": "h1",
                "source": {"table": "defer_sys_parent", "id": "p1"}
            }),
        )
        .await
        .unwrap();

    let raw = QueryCore::get_record_json("defer_sys_hist", "h1", &v)
        .await
        .unwrap()
        .expect("hist");
    PrivacyEvaluator::check_entity_read(hist, &raw, &v)
        .await
        .expect("owner read via defer");

    assert!(
        matches!(
            v.actor(),
            Actor::User { user_id } if user_id == "alice"
        ),
        "viewer actor must remain User after System parent fetch; got {:?}",
        v.actor()
    );
}

// Silence unused import warning if PUBLIC_READ unused in some builds.
#[allow(dead_code)]
fn _keep_public_read() -> &'static valence_core::privacy::PrivacyRule {
    &PUBLIC_READ
}
