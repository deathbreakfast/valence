//! Per-`Valence` host ports read from inside a privacy evaluator.

use std::any::Any;
use std::sync::Arc;
use valence_backend_mem::InMemoryBackend;
use valence_core::{
    Actor, ActorContext, JsonActorContext, PolicyEvaluator, PrivacyOperation, Valence,
};

#[derive(Debug)]
struct BotGate {
    allow: bool,
}

#[derive(Debug)]
struct GateEvaluator;

#[async_trait::async_trait]
impl PolicyEvaluator for GateEvaluator {
    fn name(&self) -> &'static str {
        "gate"
    }

    async fn evaluate(
        &self,
        _op: PrivacyOperation,
        _record: &serde_json::Value,
        _actor: &dyn ActorContext,
        v: &Valence,
    ) -> valence_core::Result<bool> {
        Ok(v.extension::<BotGate>().is_some_and(|g| g.allow))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn valence_with(gate: Option<BotGate>) -> Valence {
    let mut b = Valence::builder().add_backend("default", Arc::new(InMemoryBackend::new()));
    if let Some(g) = gate {
        b = b.extension(Arc::new(g));
    }
    b.build().expect("build")
}

#[test]
fn registered_port_is_returned_and_missing_is_none() {
    let v = valence_with(Some(BotGate { allow: true }));
    assert!(v.extension::<BotGate>().expect("registered").allow);
    assert!(v.extension::<String>().is_none());
    assert!(valence_with(None).extension::<BotGate>().is_none());
}

#[test]
fn later_registration_of_a_type_wins() {
    let v = Valence::builder()
        .add_backend("default", Arc::new(InMemoryBackend::new()))
        .extension(Arc::new(BotGate { allow: false }))
        .extension(Arc::new(BotGate { allow: true }))
        .build()
        .expect("build");
    assert!(v.extension::<BotGate>().expect("registered").allow);
}

#[test]
fn derived_handles_keep_ports() {
    let v = valence_with(Some(BotGate { allow: true }));
    let as_user = v.with_actor(Actor::Anonymous);
    assert!(as_user.extension::<BotGate>().is_some());
}

#[test]
fn runtimes_do_not_share_ports() {
    let open = valence_with(Some(BotGate { allow: true }));
    let closed = valence_with(Some(BotGate { allow: false }));
    assert!(open.extension::<BotGate>().expect("open").allow);
    assert!(!closed.extension::<BotGate>().expect("closed").allow);
}

#[tokio::test]
async fn evaluator_decides_from_the_runtime_port() {
    let actor = JsonActorContext::new(serde_json::json!("Anonymous"));
    let rec = serde_json::json!({});
    let op = PrivacyOperation::Read;
    let eval = GateEvaluator;
    for (gate, want) in [
        (Some(BotGate { allow: true }), true),
        (Some(BotGate { allow: false }), false),
        (None, false),
    ] {
        let v = valence_with(gate);
        assert_eq!(eval.evaluate(op, &rec, &actor, &v).await.unwrap(), want);
    }
}
