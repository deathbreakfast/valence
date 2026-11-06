//! `edge_policy: SourceUpdateTargetRead` on ManyToMany edge writes (FU-16).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::error::Error;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy::PrivacyRule;
use valence_core::privacy_policies::common::AUTHENTICATED;
use valence_core::privacy_policies::owner::OWNER_BY_USER_FIELD;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaConnection, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule,
    SchemaPolicyRules, SchemaPrivacy,
};
use valence_core::{DatabaseBackend, DatabaseEvaluator, RecordId, Valence};

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Links fixture rows so the Valence suite can check edge privacy on relate and unrelate. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

fn field(name: &str, primary: bool) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        field_type: "string".to_string(),
        primary,
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

fn rules(name: &str, eval: &'static PrivacyRule) -> Option<SchemaPolicyRules> {
    Some(SchemaPolicyRules {
        allow: vec![SchemaPolicyRule {
            name: name.to_string(),
            description: None,
            evaluator: Some(eval),
        }],
        ..SchemaPolicyRules::default()
    })
}

fn schema(
    table: &str,
    policies: SchemaPolicies,
    connections: Vec<SchemaConnection>,
) -> &'static SchemaMetadata {
    let schema: &'static Schema = Box::leak(Box::new(Schema {
        name: table.to_string(),
        version: "0.1.0".to_string(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".to_string(),
            write: "test".to_string(),
        },
        policies: Some(policies),
        fields: vec![field("id", true), field("user", false)],
        edges: Vec::new(),
        connections,
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        retired_fields: Vec::new(),
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,
            repository: "https://github.com/unified-field-dev/valence".to_string(),
        },
    }));
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

/// Tables for one test: a doc (any signed-in user reads, only its owner updates) linked
/// to labels (only their owner reads) through `edge`.
struct Fixture {
    doc: String,
    label: String,
    edge: String,
    backend: Arc<dyn DatabaseBackend>,
}

impl Fixture {
    async fn new(suffix: &str, edge_policy: Option<&str>) -> Self {
        let doc = format!("edge_doc_{suffix}");
        let label = format!("edge_label_{suffix}");
        let edge = format!("edge_doc_label_{suffix}");
        let doc_policies = SchemaPolicies {
            read: rules("AUTH", &AUTHENTICATED),
            create: rules("AUTH", &AUTHENTICATED),
            update: rules("OWNER", &OWNER_BY_USER_FIELD),
            delete: rules("OWNER", &OWNER_BY_USER_FIELD),
        };
        let label_policies = SchemaPolicies {
            read: rules("OWNER", &OWNER_BY_USER_FIELD),
            create: rules("AUTH", &AUTHENTICATED),
            update: rules("OWNER", &OWNER_BY_USER_FIELD),
            delete: rules("OWNER", &OWNER_BY_USER_FIELD),
        };
        let labels = SchemaConnection {
            name: "labels".to_string(),
            from_table: doc.clone(),
            from_field: "labels".to_string(),
            to_table: label.clone(),
            cardinality: "ManyToMany".to_string(),
            required: false,
            on_delete: "Cascade".to_string(),
            label: "labels".to_string(),
            model_path: None,
            reverse_field: None,
            edge_table: Some(edge.clone()),
            edge_policy: edge_policy.map(str::to_string),
            target_trait: None,
        };
        SchemaRegistry::register_overlay(schema(&doc, doc_policies, vec![labels]));
        SchemaRegistry::register_overlay(schema(&label, label_policies, Vec::new()));

        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        backend
            .create_record(&doc, serde_json::json!({"id": "d1", "user": "alice"}))
            .await
            .unwrap();
        for (id, owner) in [("mine", "alice"), ("bobs", "bob")] {
            backend
                .create_record(&label, serde_json::json!({"id": id, "user": owner}))
                .await
                .unwrap();
        }
        Self {
            doc,
            label,
            edge,
            backend,
        }
    }

    fn checked(suffix: &str) -> impl std::future::Future<Output = Self> + '_ {
        Self::new(
            suffix,
            Some(SchemaConnection::EDGE_POLICY_SOURCE_UPDATE_TARGET_READ),
        )
    }

    fn as_user(&self, user_id: &str) -> Valence {
        Valence::builder()
            .add_backend("default", Arc::clone(&self.backend))
            .with_actor(Actor::User {
                user_id: user_id.into(),
            })
            .build()
            .expect("build")
    }

    fn doc_id(&self) -> RecordId {
        RecordId::new(&self.doc, "d1")
    }

    fn label_id(&self, id: &str) -> RecordId {
        RecordId::new(&self.label, id)
    }

    async fn targets(&self) -> Vec<RecordId> {
        self.backend
            .get_edge_targets(&self.doc_id(), &self.edge)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn edge_policy_reader_cannot_relate_sad() {
    let fx = Fixture::checked("reader").await;
    // Bob can read the doc (AUTHENTICATED) but only alice may update it.
    let bob = fx.as_user("bob");
    let err = bob
        .relate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("bobs"), purpose())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
    assert_eq!(fx.targets().await, Vec::<RecordId>::new());
}

#[tokio::test]
async fn edge_policy_target_unreadable_sad() {
    let fx = Fixture::checked("unreadable").await;
    let alice = fx.as_user("alice");
    let err = alice
        .relate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("bobs"), purpose())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
    assert_eq!(fx.targets().await, Vec::<RecordId>::new());
}

#[tokio::test]
async fn edge_policy_missing_row_not_found_sad() {
    let fx = Fixture::checked("missing").await;
    let alice = fx.as_user("alice");

    let missing_target = alice
        .relate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("gone"), purpose())
        .await
        .unwrap_err();
    assert!(
        matches!(missing_target, Error::NotFound(_)),
        "{missing_target:?}"
    );

    let missing_source = alice
        .relate_edge(
            &fx.edge,
            &RecordId::new(&fx.doc, "gone"),
            &fx.label_id("mine"),
            purpose(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(missing_source, Error::NotFound(_)),
        "{missing_source:?}"
    );

    let unknown_target_table = alice
        .relate_edge(
            &fx.edge,
            &fx.doc_id(),
            &RecordId::new("edge_no_such_table", "x"),
            purpose(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(unknown_target_table, Error::NotFound(_)),
        "{unknown_target_table:?}"
    );

    assert_eq!(fx.targets().await, Vec::<RecordId>::new());
}

#[tokio::test]
async fn edge_policy_editor_relates_and_unrelates_happy() {
    let fx = Fixture::checked("editor").await;
    let alice = fx.as_user("alice");
    alice
        .relate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("mine"), purpose())
        .await
        .expect("owner links a label they can read");
    assert_eq!(fx.targets().await, vec![fx.label_id("mine")]);

    // A reader of the doc can't strip alice's link either.
    let bob = fx.as_user("bob");
    let err = bob
        .unrelate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("mine"), purpose())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Privacy(_)), "{err:?}");
    assert_eq!(fx.targets().await, vec![fx.label_id("mine")]);

    alice
        .unrelate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("mine"), purpose())
        .await
        .expect("owner unlinks");
    assert_eq!(fx.targets().await, Vec::<RecordId>::new());
}

#[tokio::test]
async fn edge_policy_absent_keeps_raw_write_happy() {
    let fx = Fixture::new("absent", None).await;
    // Without edge_policy, even a reader linking an unreadable target goes through.
    let carol = fx.as_user("carol");
    carol
        .relate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("bobs"), purpose())
        .await
        .expect("raw edge write");
    assert_eq!(fx.targets().await, vec![fx.label_id("bobs")]);
    carol
        .unrelate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("bobs"), purpose())
        .await
        .expect("raw edge delete");
    assert_eq!(fx.targets().await, Vec::<RecordId>::new());
}

#[tokio::test]
async fn edge_policy_unknown_value_refused_sad() {
    let fx = Fixture::new("unknown", Some("AnyoneMayLink")).await;
    let alice = fx.as_user("alice");
    let err = alice
        .relate_edge(&fx.edge, &fx.doc_id(), &fx.label_id("mine"), purpose())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "{err:?}");
    assert_eq!(fx.targets().await, Vec::<RecordId>::new());
}
