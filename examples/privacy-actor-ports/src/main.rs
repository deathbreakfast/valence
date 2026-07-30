//! Host-injectable ports (`SecretProvider`, `ActorFactory`, `DatabaseEndpointResolver`) plus the
//! `Actor` + schema `policies:` privacy deny/allow contract, side by side.
//!
//! ```bash
//! cargo run -p privacy-actor-ports
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    Actor, DatabaseBackend, EnvSecretProvider, InMemoryBackend, JsonActorFactory, PrivacyEvaluator,
    PrivacyOperation, SchemaRegistry, StaticEndpointResolver, Valence,
};

// Step 1 — Schema-level `policies:` are the privacy contract. Each operation
// gets its own allow-list; empty/absent lists default-deny every actor except `Actor::System`.
valence::valence_schema! {
    Note {
        table: "note",
        version: "0.1.0",
        description: "Private note owned by a single user",
        policies: {
            read: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            delete: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
            body: { r#type: FieldType::String, required: true },
        ],
    }
}

#[tokio::main]
async fn main() -> valence::Result<()> {
    std::env::set_var("PRIVACY_ACTOR_PORTS_API_KEY", "sk-demo-123");

    // Step 2 — Wire the three host-injectable ports on the builder: secrets, identity, endpoints.
    // Each is a trait a host implements and hands over as `Arc<dyn _>` — never a global Mode enum.
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", Arc::clone(&backend))
        .secret_provider(Arc::new(EnvSecretProvider))
        .actor_factory(Arc::new(JsonActorFactory))
        .endpoint_resolver(Arc::new(StaticEndpointResolver::new(vec![(
            "notes-db",
            "postgres://db.example/notes",
        )])))
        .with_actor(Actor::System {
            operation: "bootstrap".into(),
        })
        .build()?;

    // Step 3 — SecretProvider port: hosts implement Vault/KMS adapters; `EnvSecretProvider` is the
    // reference impl shipped for tests/examples (looks up a process env var by key).
    let api_key = valence
        .secret_provider()
        .get_secret("PRIVACY_ACTOR_PORTS_API_KEY")?;
    println!("privacy-actor-ports: secret_provider -> {api_key:?}");
    assert_eq!(api_key.as_deref(), Some("sk-demo-123"));

    // Step 4 — DatabaseEndpointResolver port: bootstrap-time physical URL lookup for a logical
    // name, distinct from schema `database:` router-key selection (see `ports::endpoints` docs).
    let url = valence.endpoint_resolver().resolve_url("notes-db")?;
    println!("privacy-actor-ports: endpoint_resolver -> {url:?}");
    assert_eq!(url.as_deref(), Some("postgres://db.example/notes"));

    // Step 5 — ActorFactory port: builds an opaque `ActorContext` from host-supplied JSON at the
    // request boundary. Typed actor enums (the `Actor` used below) stay entirely host-side.
    let ctx = valence
        .actor_factory()
        .build(&serde_json::json!({"kind": "user", "id": "alice"}))?;
    println!(
        "privacy-actor-ports: actor_factory -> {:?}",
        ctx.actor_json()
    );

    // Step 6 — Seed two notes as System (the bootstrap actor bypasses privacy checks entirely).
    backend
        .create_record(
            "note",
            serde_json::json!({"id": "n1", "user": "alice", "body": "alice's private note"}),
        )
        .await?;
    backend
        .create_record(
            "note",
            serde_json::json!({"id": "n2", "user": "bob", "body": "bob's private note"}),
        )
        .await?;
    let alice_note = backend
        .get_record("note", "n1")
        .await?
        .expect("n1 was just created");

    let schema = SchemaRegistry::global()
        .get_schema("note")
        .expect("note schema registered by valence_schema!");

    // Step 7 — `Valence::with_actor` clones the handle with a different actor; `PrivacyEvaluator`
    // reads that actor off the handle it's given, not from a bare `Actor` argument.
    // Allow: alice reads her own note (OWNER_BY_USER_FIELD matches the "user" field).
    let as_alice = valence.with_actor(Actor::User {
        user_id: "alice".into(),
    });
    PrivacyEvaluator::check_entity_read(schema, &alice_note, &as_alice).await?;
    println!("privacy-actor-ports: allow — alice reads her own note");

    // Step 8 — Deny: bob cannot read alice's note; anonymous cannot read anyone's note.
    let as_bob = valence.with_actor(Actor::User {
        user_id: "bob".into(),
    });
    let bob_denied = PrivacyEvaluator::check_entity_read(schema, &alice_note, &as_bob).await;
    let bob_err = bob_denied.expect_err("bob must not read alice's note");
    println!("privacy-actor-ports: deny — bob cannot read alice's note ({bob_err})");

    let as_anon = valence.with_actor(Actor::Anonymous);
    let anon_denied = PrivacyEvaluator::check_entity_read(schema, &alice_note, &as_anon)
        .await
        .expect_err("anonymous must not read any note");
    println!("privacy-actor-ports: deny — anonymous cannot read alice's note ({anon_denied})");

    // Step 9 — Allow/deny on create: AUTHENTICATED lets any signed-in user (or System) create;
    // anonymous is rejected before a row is ever written.
    let new_note = serde_json::json!({"id": "n3", "user": "carol", "body": "carol's note"});
    let as_carol = valence.with_actor(Actor::User {
        user_id: "carol".into(),
    });
    PrivacyEvaluator::check_entity_access(schema, PrivacyOperation::Create, &new_note, &as_carol)
        .await?;
    println!("privacy-actor-ports: allow — carol (authenticated) may create a note");
    PrivacyEvaluator::check_entity_access(schema, PrivacyOperation::Create, &new_note, &as_anon)
        .await
        .expect_err("anonymous create must be denied");

    // Step 10 — Allow/deny on delete: SYSTEM_ONLY means even the owner cannot delete — only
    // System can. This is the "privacy rule beats ownership" lesson.
    PrivacyEvaluator::check_entity_access(schema, PrivacyOperation::Delete, &alice_note, &as_alice)
        .await
        .expect_err("owner alone cannot delete a SYSTEM_ONLY-guarded note");
    let as_system = valence.with_actor(Actor::System {
        operation: "cleanup".into(),
    });
    PrivacyEvaluator::check_entity_access(
        schema,
        PrivacyOperation::Delete,
        &alice_note,
        &as_system,
    )
    .await?;
    println!("privacy-actor-ports: allow — System may delete; owner alone may not");

    println!("privacy-actor-ports: OK (ports wired, allow/deny both proven)");
    Ok(())
}
