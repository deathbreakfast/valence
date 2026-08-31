//! Defer-to-edge read privacy: satellite history inherits parent Read.
//!
//! ```bash
//! cargo run -p privacy-defer-to-edge
//! ```
//!
//! Success line: `privacy-defer-to-edge: OK — owner allow + stranger deny`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::sync::Arc;

use valence::{
    Actor, DatabaseBackend, InMemoryBackend, PrivacyEvaluator, QueryCore, SchemaRegistry, Valence,
};

valence::valence_schema! {
    DeferDemoParent {
        table: "defer_demo_parent",
        version: "0.1.0",
        description: "Owner-scoped parent for defer-to-edge demo",
        policies: {
            read: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            delete: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
        ],
    }
}

valence::valence_schema! {
    DeferDemoHistory {
        table: "defer_demo_history",
        version: "0.1.0",
        description: "Satellite history that defers read to parent source",
        policies: {
            read: {
                always_allow: [valence::privacy_policies::common::SYSTEM_ONLY],
                defer_to_edge: "source",
            },
            create: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] },
            update: { always_block: [valence::privacy_policies::common::BLOCK_ALL] },
            delete: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            source: { r#type: FieldType::Record("defer_demo_parent"), required: true },
        ],
        connections: [
            source: {
                table: "defer_demo_parent",
                cardinality: HasOne,
                required: true,
                on_delete: Cascade,
            },
        ],
    }
}

#[tokio::main]
async fn main() -> valence::Result<()> {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    let sys = Valence::builder()
        .add_backend("default", Arc::clone(&backend))
        .with_actor(Actor::System {
            operation: "bootstrap".into(),
        })
        .build()?;

    backend
        .create_record(
            "defer_demo_parent",
            serde_json::json!({"id": "p1", "user": "alice"}),
        )
        .await?;
    backend
        .create_record(
            "defer_demo_history",
            serde_json::json!({
                "id": "h1",
                "source": {"table": "defer_demo_parent", "id": "p1"}
            }),
        )
        .await?;

    let hist_schema = SchemaRegistry::global()
        .get_schema("defer_demo_history")
        .expect("history schema registered");
    let hist_raw = QueryCore::get_record_json("defer_demo_history", "h1", &sys)
        .await?
        .expect("history row");

    let alice = sys.with_actor(Actor::User {
        user_id: "alice".into(),
    });
    PrivacyEvaluator::check_entity_read(hist_schema, &hist_raw, &alice).await?;

    let bob = sys.with_actor(Actor::User {
        user_id: "bob".into(),
    });
    let deny = PrivacyEvaluator::check_entity_read(hist_schema, &hist_raw, &bob).await;
    assert!(deny.is_err(), "stranger must be denied");

    println!("privacy-defer-to-edge: OK — owner allow + stranger deny");
    let _ = sys;
    Ok(())
}
