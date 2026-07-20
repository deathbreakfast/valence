//! Stable owner identity for row-level ownership metadata.

use serde::{Deserialize, Serialize};

use crate::Actor;

/// Kind of principal that owns a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    User,
    Account,
    Application,
    System,
    Service,
}

impl OwnerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OwnerKind::User => "user",
            OwnerKind::Account => "account",
            OwnerKind::Application => "application",
            OwnerKind::System => "system",
            OwnerKind::Service => "service",
        }
    }
}

/// Resolved owner for a row (stored in ownership tables at the host layer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerRef {
    pub owner_id: String,
    pub owner_kind: OwnerKind,
}

impl OwnerRef {
    #[must_use]
    pub fn system() -> Self {
        Self {
            owner_id: "system".to_string(),
            owner_kind: OwnerKind::System,
        }
    }

    #[must_use]
    pub fn from_actor(actor: &Actor) -> Self {
        match actor {
            Actor::User { user_id } => Self {
                owner_id: user_id.clone(),
                owner_kind: OwnerKind::User,
            },
            Actor::ServiceUser { service_name } => Self {
                owner_id: service_name.clone(),
                owner_kind: OwnerKind::Service,
            },
            Actor::System { operation } => Self {
                owner_id: operation.clone(),
                owner_kind: OwnerKind::System,
            },
            Actor::Anonymous => Self {
                owner_id: "anonymous".to_string(),
                owner_kind: OwnerKind::System,
            },
        }
    }
}

/// Ownership behavior declared in schema DSL (`ownership: { ... }`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OwnershipConfig {
    #[serde(default)]
    pub system_owned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{OwnerKind, OwnerRef};
    use crate::Actor;

    #[test]
    fn owner_ref_from_user_actor() {
        let r = OwnerRef::from_actor(&Actor::User {
            user_id: "u42".into(),
        });
        assert_eq!(r.owner_id, "u42");
        assert_eq!(r.owner_kind, OwnerKind::User);
    }
}
