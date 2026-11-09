//! Privacy policy port and schema-attached evaluators.

use crate::actor::Actor;
use crate::error::Result;
use crate::ports::actor::ActorContext;
use crate::privacy::types::{PrivacyOperation, PrivacyRule};
use crate::runtime::Valence;
use async_trait::async_trait;
use std::any::Any;

/// Per-rule evaluator referenced from schema metadata (not registered on the builder).
///
/// Attach consts in `valence_schema!` `policies:` blocks. See [`crate::privacy`].
///
/// An evaluator that needs a host port (a bot gate, say) reads it from the `&Valence` it is
/// given with [`Valence::extension`]; the host registers it per runtime with
/// [`crate::ValenceBuilder::extension`].
#[async_trait]
pub trait PolicyEvaluator: Send + Sync + std::fmt::Debug + Any + 'static {
    fn name(&self) -> &'static str;

    fn description(&self) -> Option<&'static str> {
        None
    }

    async fn evaluate(
        &self,
        _op: PrivacyOperation,
        _record: &serde_json::Value,
        _actor: &dyn ActorContext,
        _v: &Valence,
    ) -> Result<bool> {
        Ok(true)
    }

    fn as_any(&self) -> &dyn Any;
}

fn actor_from_context(actor: &dyn ActorContext) -> Result<Actor> {
    serde_json::from_value(actor.actor_json().clone())
        .map_err(|e| crate::error::Error::Internal(format!("invalid actor context: {e}")))
}

#[async_trait]
impl PolicyEvaluator for PrivacyRule {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> Option<&'static str> {
        self.description
    }

    async fn evaluate(
        &self,
        _op: PrivacyOperation,
        record: &serde_json::Value,
        actor: &dyn ActorContext,
        _v: &Valence,
    ) -> Result<bool> {
        let viewer = actor_from_context(actor)?;
        Ok((self.check)(record, &viewer))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
