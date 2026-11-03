//! Reusable [`PrivacyRule`] definitions for schema codegen.

use crate::actor::Actor;
use crate::privacy::PrivacyRule;
use serde_json::Value as JsonValue;

use super::checks::{
    authenticated_check, block_all_check, owner_by_id_check, owner_by_user_field_check,
    public_read_check, system_only_check,
};

/// Common privacy policies that can be reused across schemas
pub mod common {
    use super::{
        authenticated_check, block_all_check, public_read_check, system_only_check, PrivacyRule,
    };

    pub const PUBLIC_READ: PrivacyRule = PrivacyRule {
        name: "public_read",
        description: Some("Allow public read access (anyone can read)"),
        check: public_read_check,
    };

    /// Allows any non-anonymous actor.
    ///
    /// That includes signed-in users, [`crate::actor::Actor::System`], and
    /// [`crate::actor::Actor::ServiceUser`] — a service user is authenticated by definition.
    /// To require a human user instead, write a custom [`PrivacyRule`] whose `check` is
    /// `viewer.is_user()` on its own, rather than reusing `AUTHENTICATED`.
    ///
    /// ```
    /// use valence_core::actor::Actor;
    /// use valence_core::privacy_policies::common::AUTHENTICATED;
    ///
    /// let service_user = Actor::ServiceUser {
    ///     service_name: "svc:ingest".into(),
    /// };
    /// assert!((AUTHENTICATED.check)(&serde_json::json!({}), &service_user));
    /// assert!(!(AUTHENTICATED.check)(&serde_json::json!({}), &Actor::Anonymous));
    /// ```
    pub const AUTHENTICATED: PrivacyRule = PrivacyRule {
        name: "authenticated",
        description: Some("Allow authenticated users (not anonymous)"),
        check: authenticated_check,
    };

    pub const SYSTEM_ONLY: PrivacyRule = PrivacyRule {
        name: "system_only",
        description: Some("Allow only system actors"),
        check: system_only_check,
    };

    pub const BLOCK_ALL: PrivacyRule = PrivacyRule {
        name: "block_all",
        description: Some("Block all access"),
        check: block_all_check,
    };
}

/// Helper module for owner-based policies
pub mod owner {
    use super::{owner_by_id_check, owner_by_user_field_check, Actor, JsonValue, PrivacyRule};

    /// Owner rule keyed on the record's `id` field.
    ///
    /// A [`crate::actor::Actor::ServiceUser`] never passes it: a service user has no user id
    /// to compare. See [`check_owner_by_field`].
    pub fn owner_by_id() -> PrivacyRule {
        PrivacyRule {
            name: "owner_by_id",
            description: Some("Owner check via \"id\" field"),
            check: owner_by_id_check,
        }
    }

    /// True when `viewer` is [`Actor::System`], or a [`Actor::User`] whose id equals the
    /// record's `field` value (a `table:` prefix on either side is ignored).
    ///
    /// A [`Actor::ServiceUser`] (a bot) never matches, because it has no user id. To let a
    /// specific bot through, add a custom rule beside the owner rule that checks
    /// `viewer.service_name()`, such as `== Some("svc:ingest")`. `common::SYSTEM_ONLY` does
    /// not admit service users, and `common::AUTHENTICATED` would also open the records to
    /// every signed-in user, which defeats the owner rule.
    ///
    /// ```
    /// use valence_core::actor::Actor;
    /// use valence_core::privacy_policies::owner::check_owner_by_field;
    ///
    /// let record = serde_json::json!({ "user": "user:alice" });
    /// let alice = Actor::User { user_id: "alice".into() };
    /// let bot = Actor::ServiceUser { service_name: "svc:ingest".into() };
    ///
    /// let system = Actor::System { operation: "job".into() };
    ///
    /// assert!(check_owner_by_field(&record, &alice, "user"));
    /// assert!(check_owner_by_field(&record, &system, "user"));
    /// assert!(!check_owner_by_field(&record, &bot, "user"));
    /// ```
    pub fn check_owner_by_field(record: &JsonValue, viewer: &Actor, field: &str) -> bool {
        if viewer.is_system() {
            return true;
        }

        if let Some(viewer_id) = viewer.user_id() {
            if let Some(field_value) = record.get(field) {
                let owner_id_owned: String;
                let owner_id: &str = if let Some(field_str) = field_value.as_str() {
                    if let Some(id_part) = field_str.split(':').nth(1) {
                        id_part
                    } else {
                        field_str
                    }
                } else if let Some(obj) = field_value.as_object() {
                    if let Some(id_val) = obj.get("id") {
                        if let Some(id_str) = id_val.as_str() {
                            // Match string-field normalization: strip a `table:` prefix when present.
                            owner_id_owned = if let Some(id_part) = id_str.split(':').nth(1) {
                                id_part.to_string()
                            } else {
                                id_str.to_string()
                            };
                            &owner_id_owned
                        } else if let Some(id_obj) = id_val.as_object() {
                            if let Some(inner) = id_obj.get("String").and_then(|v| v.as_str()) {
                                owner_id_owned = if let Some(id_part) = inner.split(':').nth(1) {
                                    id_part.to_string()
                                } else {
                                    inner.to_string()
                                };
                                &owner_id_owned
                            } else {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                };

                let normalized_viewer_id = if let Some(id_part) = viewer_id.split(':').nth(1) {
                    id_part
                } else {
                    viewer_id
                };
                return normalized_viewer_id == owner_id;
            }
        }
        false
    }

    /// Owner rule keyed on the record's `user` field. Never matches a
    /// [`Actor::ServiceUser`]; see [`check_owner_by_field`].
    pub const OWNER_BY_USER_FIELD: PrivacyRule = PrivacyRule {
        name: "owner_by_user_field",
        description: Some("Owner check via \"user\" field"),
        check: owner_by_user_field_check,
    };

    /// Owner rule keyed on the record's `id` field. Never matches a
    /// [`Actor::ServiceUser`]; see [`check_owner_by_field`].
    pub const OWNER_BY_ID: PrivacyRule = PrivacyRule {
        name: "owner_by_id",
        description: Some("Owner check via \"id\" field"),
        check: owner_by_id_check,
    };
}

/// Helper functions for policy evaluation in custom rules.
pub mod helpers {
    use crate::actor::Actor;

    /// False for any actor without a user id, including a service user.
    pub fn is_owner(viewer: &Actor, record_id: &str) -> bool {
        if let Some(viewer_id) = viewer.user_id() {
            viewer_id == record_id
        } else {
            false
        }
    }

    /// False for any actor without a user id, including a service user.
    pub fn is_owner_via_field(viewer: &Actor, owner_field_value: &str) -> bool {
        let owner_id = if let Some(id_part) = owner_field_value.split(':').nth(1) {
            id_part
        } else {
            owner_field_value
        };

        if let Some(viewer_id) = viewer.user_id() {
            viewer_id == owner_id
        } else {
            false
        }
    }
}
