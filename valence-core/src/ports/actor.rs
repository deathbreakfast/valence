//! Opaque actor context at the port — typed identity adapters live in host crates.

use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

/// Request-scoped identity view used by privacy and ownership checks.
pub trait ActorContext: Send + Sync {
    /// Opaque JSON payload representing the actor (shape is host-defined).
    fn actor_json(&self) -> &Value;
}

/// [`ActorContext`] that stores JSON as-is.
pub struct JsonActorContext {
    json: Value,
}

impl JsonActorContext {
    /// Wrap an opaque actor JSON value.
    #[must_use]
    pub fn new(json: Value) -> Self {
        Self { json }
    }
}

impl ActorContext for JsonActorContext {
    fn actor_json(&self) -> &Value {
        &self.json
    }
}

/// Build an [`ActorContext`] from opaque JSON at the Valence port boundary.
///
/// Typed product actor enums stay in host crates; this port only sees JSON.
/// Wire with [`crate::ValenceBuilder::actor_factory`].
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use valence_core::{ActorFactory, JsonActorFactory};
///
/// let factory = JsonActorFactory;
/// let ctx = factory
///     .build(&serde_json::json!({"kind": "user", "id": "u1"}))
///     .expect("build");
/// assert_eq!(ctx.actor_json()["id"], "u1");
/// let _ = Arc::new(factory);
/// ```
#[async_trait]
pub trait ActorFactory: Send + Sync {
    /// Construct a context from host-supplied actor JSON.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    fn build(&self, actor_json: &Value) -> Result<Arc<dyn ActorContext>>;
}

/// Reference factory that wraps JSON in [`JsonActorContext`].
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonActorFactory;

#[async_trait]
impl ActorFactory for JsonActorFactory {
    fn build(&self, actor_json: &Value) -> Result<Arc<dyn ActorContext>> {
        Ok(Arc::new(JsonActorContext::new(actor_json.clone())))
    }
}
