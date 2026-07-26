//! Optional policy for validating opaque `actor_json` at factory build time.
//!
//! Hosts that reconstruct [`crate::actor::Actor`] from external JSON should install an
//! [`ActorJsonPolicy`] so untrusted clients cannot mint [`crate::actor::Actor::System`].

use serde_json::Value;

use crate::error::{Error, Result};

/// Trust level for an actor JSON call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorTrust {
    /// In-process / bootstrapped callers (may mint elevated actor shapes when policy allows).
    Internal,
    /// Externally reachable surfaces (HTTP, jobs with client-supplied JSON, etc.).
    External,
}

/// Validates `actor_json` before a [`crate::runtime::ValenceFactory`] binds an actor.
///
/// # Errors
///
/// Implementations return [`Error::Validation`] when the actor is rejected.
pub trait ActorJsonPolicy: Send + Sync {
    /// Validate actor JSON for the given trust level.
    ///
    /// # Errors
    ///
    /// Returns an error when the actor must not be bound.
    fn validate(&self, trust: ActorTrust, actor_json: &Value) -> Result<()>;
}

/// Rejects well-known System-shaped actors on [`ActorTrust::External`] paths.
///
/// Recognizes `{"System": ...}` object keys (case-sensitive), matching [`crate::actor::Actor`].
#[derive(Debug, Default, Clone, Copy)]
pub struct RejectExternalSystemActor;

impl ActorJsonPolicy for RejectExternalSystemActor {
    fn validate(&self, trust: ActorTrust, actor_json: &Value) -> Result<()> {
        if trust == ActorTrust::External && is_system_shaped_actor(actor_json) {
            return Err(Error::Validation(
                "external actor_json cannot use System-shaped actor".into(),
            ));
        }
        Ok(())
    }
}

/// True when `actor_json` uses the well-known System object key.
#[must_use]
pub fn is_system_shaped_actor(actor_json: &Value) -> bool {
    actor_json.get("System").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_external_system() {
        let policy = RejectExternalSystemActor;
        let system = serde_json::json!({"System": {"operation": "x"}});
        assert!(policy
            .validate(ActorTrust::External, &system)
            .unwrap_err()
            .to_string()
            .contains("System"));
        assert!(policy.validate(ActorTrust::Internal, &system).is_ok());
    }

    #[test]
    fn allow_external_user() {
        let policy = RejectExternalSystemActor;
        let user = serde_json::json!({"User": {"user_id": "u1"}});
        assert!(!is_system_shaped_actor(&user));
        assert!(policy.validate(ActorTrust::External, &user).is_ok());
    }
}
