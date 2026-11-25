//! `QueryCore::get_entity_read` tells hidden records apart from missing ones (in-memory).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::entity::EntityRead;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy::PrivacyRule;
use valence_core::privacy_policies::common::{AUTHENTICATED, SYSTEM_ONLY};
use valence_core::query::QueryCore;
use valence_core::schema::SchemaMetadata;
use valence_core::schema_api::{
    Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
use valence_core::{DatabaseBackend, DatabaseEvaluator, Valence};

const TABLE: &str = "er_doc";

/// Blocks every row whose `archived` is `true`.
static ER_ARCHIVED: PrivacyRule = PrivacyRule {
    name: "ER_ARCHIVED",
    description: None,
    check: |record, _viewer| record.get("archived").and_then(|a| a.as_bool()) == Some(true),
};

fn rule(name: &str, evaluator: &'static PrivacyRule) -> SchemaPolicyRule {
    SchemaPolicyRule {
        name: name.into(),
        description: None,
        evaluator: Some(evaluator),
    }
}

fn field(name: &str, policies: Option<SchemaPolicies>) -> SchemaField {
    SchemaField {
        name: name.into(),
        field_type: "String".into(),
        primary: name == "id",
        nullable: name != "id",
        indexed: false,
        unique: false,
        default: None,
        fk: None,
        validations: vec![],
        policies,
        encrypted: false,
        enum_variants: vec![],
        enum_type: None,
        model_path: None,
    }
}

fn er_schema() -> &'static SchemaMetadata {
    let system_only_read = SchemaPolicies {
        read: Some(SchemaPolicyRules {
            allow: vec![rule("SYSTEM_ONLY", &SYSTEM_ONLY)],
            ..SchemaPolicyRules::default()
        }),
        ..SchemaPolicies::default()
    };
    let schema = Box::leak(Box::new(Schema {
        name: TABLE.into(),
        version: "0.1.0".into(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "t".into(),
            write: "t".into(),
        },
        policies: Some(SchemaPolicies {
            read: Some(SchemaPolicyRules {
                always_block: vec![rule("ER_ARCHIVED", &ER_ARCHIVED)],
                allow: vec![rule("AUTHENTICATED", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![
            field("id", None),
            field("title", None),
            field("archived", None),
            field("secret", Some(system_only_read)),
        ],
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        retired_fields: Vec::new(),
        ttl: None,
        partition_export: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "1d".into(),
            row_count: 0,
            owner: "t".into(),
            description: None,
            repository: "https://github.com/unified-field-dev/valence".to_string(),
        },
    }));
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(er_schema)
}

async fn seeded(actor: Actor) -> Valence {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    for row in [
        json!({"id": "open", "title": "Open doc", "archived": false, "secret": "s3cret"}),
        json!({"id": "archived", "title": "Old doc", "archived": true}),
    ] {
        backend.create_record(TABLE, row).await.unwrap();
    }
    Valence::builder()
        .add_backend("default", backend)
        .with_actor(actor)
        .build()
        .expect("build")
}

fn user() -> Actor {
    Actor::User {
        user_id: "u1".into(),
    }
}

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Loads fixture records to check that an admin read reveals whether a hidden record exists. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

#[tokio::test]
async fn readable_record_is_found_with_hidden_field_policy() {
    let v = seeded(user()).await;
    let EntityRead::Found(entity) = QueryCore::get_entity_read(TABLE, "open", &v, purpose())
        .await
        .unwrap()
    else {
        panic!("expected Found");
    };
    assert_eq!(entity.data.get("title"), Some(&json!("Open doc")));
    assert!(!entity.data.contains_key("secret"));
    assert_eq!(entity.hidden_fields, vec!["secret".to_string()]);
    assert_eq!(
        entity
            .hidden_field_policies
            .get("secret")
            .map(String::as_str),
        Some("SYSTEM_ONLY")
    );
}

#[tokio::test]
async fn blocked_record_is_hidden_with_matching_rule_name() {
    let v = seeded(user()).await;
    let read = QueryCore::get_entity_read(TABLE, "archived", &v, purpose())
        .await
        .unwrap();
    assert!(
        matches!(&read, EntityRead::Hidden { policy } if policy == "ER_ARCHIVED"),
        "got {read:?}"
    );
}

#[tokio::test]
async fn record_with_no_matching_allow_is_hidden_as_default_deny() {
    let v = seeded(Actor::Anonymous).await;
    let read = QueryCore::get_entity_read(TABLE, "open", &v, purpose())
        .await
        .unwrap();
    assert!(
        matches!(&read, EntityRead::Hidden { policy } if policy == "default_deny"),
        "got {read:?}"
    );
}

#[tokio::test]
async fn unknown_id_is_missing() {
    let v = seeded(user()).await;
    let read = QueryCore::get_entity_read(TABLE, "nope", &v, purpose())
        .await
        .unwrap();
    assert!(matches!(read, EntityRead::Missing), "got {read:?}");
}

#[tokio::test]
async fn get_entity_still_hides_existence() {
    let v = seeded(user()).await;
    let denied = QueryCore::get_entity(TABLE, "archived", &v, purpose())
        .await
        .unwrap();
    let missing = QueryCore::get_entity(TABLE, "nope", &v, purpose())
        .await
        .unwrap();
    assert!(denied.is_none());
    assert!(missing.is_none());
    let found = QueryCore::get_entity(TABLE, "open", &v, purpose())
        .await
        .unwrap()
        .expect("readable");
    assert!(found.hidden_field_policies.is_empty());
}
