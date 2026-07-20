//! Actor identity for privacy checks and audit trails.

use serde::{Deserialize, Serialize};

/// Who or what is performing a Valence operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Actor {
    User { user_id: String },
    ServiceUser { service_name: String },
    System { operation: String },
    Anonymous,
}

impl Actor {
    pub fn is_user(&self) -> bool {
        matches!(self, Actor::User { .. })
    }

    pub fn is_system(&self) -> bool {
        matches!(self, Actor::System { .. })
    }

    pub fn is_anonymous(&self) -> bool {
        matches!(self, Actor::Anonymous)
    }

    pub fn user_id(&self) -> Option<&str> {
        match self {
            Actor::User { user_id, .. } => Some(user_id),
            _ => None,
        }
    }

    #[must_use]
    pub fn initialize_system_context() -> Self {
        Actor::System {
            operation: "initialize_system_context".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_kinds() {
        let user = Actor::User {
            user_id: "u1".into(),
        };
        assert!(user.is_user());
        assert_eq!(user.user_id(), Some("u1"));

        let system = Actor::System {
            operation: "boot".into(),
        };
        assert!(system.is_system());

        assert!(Actor::Anonymous.is_anonymous());
    }
}
