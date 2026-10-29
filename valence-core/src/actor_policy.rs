//! Optional policy for validating opaque `actor_json` at factory build time.
//!
//! Hosts that reconstruct [`crate::actor::Actor`] from external JSON should install an
//! [`ActorJsonPolicy`] so untrusted clients cannot mint [`crate::actor::Actor::System`] or
//! [`crate::actor::Actor::ServiceUser`]. A service user, built-in or registered, never comes
//! from a browser session, so external JSON claiming one is treated as forged; build those
//! actors on the server with [`ActorTrust::Internal`] from identity the host already holds.

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

/// Rejects System- and ServiceUser-shaped actors on [`ActorTrust::External`] paths.
///
/// Recognizes the `{"System": ...}` and `{"ServiceUser": ...}` object keys (case-sensitive),
/// matching [`crate::actor::Actor`]'s serde form. External JSON may still carry a `User` or
/// `Anonymous` actor. [`ActorTrust::Internal`] callers are never rejected: that is the path for
/// actors the server builds itself, such as a registered service user resolved from a stored API
/// token.
///
/// # Examples
///
/// ```
/// use valence_core::{ActorJsonPolicy, ActorTrust, RejectExternalPrivilegedActor};
///
/// let policy = RejectExternalPrivilegedActor;
/// let registered = serde_json::json!({"ServiceUser": {"service_name": "svc:7c9e6679-7425-40de-944b-e07fc1f90ae7"}});
/// let user = serde_json::json!({"User": {"user_id": "u1"}});
///
/// assert!(policy.validate(ActorTrust::External, &registered).is_err());
/// assert!(policy.validate(ActorTrust::External, &user).is_ok());
/// // Server-built actor JSON (for example a bearer layer resolving a service user's token).
/// assert!(policy.validate(ActorTrust::Internal, &registered).is_ok());
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct RejectExternalPrivilegedActor;

/// Deprecated name for [`RejectExternalPrivilegedActor`].
#[deprecated(since = "0.1.5", note = "renamed to `RejectExternalPrivilegedActor`")]
pub use RejectExternalPrivilegedActor as RejectExternalSystemActor;

impl ActorJsonPolicy for RejectExternalPrivilegedActor {
    fn validate(&self, trust: ActorTrust, actor_json: &Value) -> Result<()> {
        if trust == ActorTrust::External
            && (is_system_shaped_actor(actor_json) || is_service_user_shaped_actor(actor_json))
        {
            return Err(Error::Validation(
                "external actor_json cannot use a System or ServiceUser actor".into(),
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

/// True when `actor_json` uses the well-known ServiceUser object key.
#[must_use]
pub fn is_service_user_shaped_actor(actor_json: &Value) -> bool {
    actor_json.get("ServiceUser").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_external_system() {
        let policy = RejectExternalPrivilegedActor;
        let system = serde_json::json!({"System": {"operation": "x"}});
        assert!(policy
            .validate(ActorTrust::External, &system)
            .unwrap_err()
            .to_string()
            .contains("System"));
        assert!(policy.validate(ActorTrust::Internal, &system).is_ok());
    }

    #[test]
    fn reject_external_service_user() {
        let policy = RejectExternalPrivilegedActor;
        let registered = serde_json::json!({"ServiceUser": {"service_name": "svc:7c9e6679-7425-40de-944b-e07fc1f90ae7"}});
        let infra = serde_json::json!({"ServiceUser": {"service_name": "metrics-ingest"}});
        for actor in [&registered, &infra] {
            let err = policy
                .validate(ActorTrust::External, actor)
                .expect_err("external service user must be rejected");
            let text = err.to_string();
            assert!(text.contains("ServiceUser"), "{text}");
            assert!(
                !text.contains("svc:"),
                "error must not echo the actor: {text}"
            );
        }
    }

    #[test]
    fn allow_internal_service_user() {
        let policy = RejectExternalPrivilegedActor;
        let registered = serde_json::json!({"ServiceUser": {"service_name": "svc:7c9e6679-7425-40de-944b-e07fc1f90ae7"}});
        assert!(policy.validate(ActorTrust::Internal, &registered).is_ok());
    }

    #[test]
    fn allow_external_anonymous() {
        let policy = RejectExternalPrivilegedActor;
        assert!(policy
            .validate(ActorTrust::External, &serde_json::json!("Anonymous"))
            .is_ok());
    }

    #[test]
    fn service_user_shape_matches_only_service_user() {
        assert!(is_service_user_shaped_actor(
            &serde_json::json!({"ServiceUser": {"service_name": "x"}})
        ));
        for other in [
            serde_json::json!({"User": {"user_id": "u1"}}),
            serde_json::json!({"System": {"operation": "x"}}),
            serde_json::json!("Anonymous"),
            serde_json::json!({"serviceuser": {"service_name": "x"}}),
        ] {
            assert!(!is_service_user_shaped_actor(&other), "{other}");
        }
    }

    #[test]
    fn allow_external_user() {
        let policy = RejectExternalPrivilegedActor;
        let user = serde_json::json!({"User": {"user_id": "u1"}});
        assert!(!is_system_shaped_actor(&user));
        assert!(policy.validate(ActorTrust::External, &user).is_ok());
    }

    #[test]
    fn deprecated_alias_still_works() {
        let policy = RejectExternalSystemActor;
        let system = serde_json::json!({"System": {"operation": "x"}});
        assert!(policy.validate(ActorTrust::External, &system).is_err());
    }
}
