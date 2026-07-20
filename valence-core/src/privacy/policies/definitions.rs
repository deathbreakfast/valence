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

    pub fn owner_by_id() -> PrivacyRule {
        PrivacyRule {
            name: "owner_by_id",
            description: Some("Owner check via \"id\" field"),
            check: owner_by_id_check,
        }
    }

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
                            owner_id_owned = id_str.to_string();
                            &owner_id_owned
                        } else if let Some(id_obj) = id_val.as_object() {
                            if let Some(inner) = id_obj.get("String").and_then(|v| v.as_str()) {
                                owner_id_owned = inner.to_string();
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

    pub const OWNER_BY_USER_FIELD: PrivacyRule = PrivacyRule {
        name: "owner_by_user_field",
        description: Some("Owner check via \"user\" field"),
        check: owner_by_user_field_check,
    };

    pub const OWNER_BY_ID: PrivacyRule = PrivacyRule {
        name: "owner_by_id",
        description: Some("Owner check via \"id\" field"),
        check: owner_by_id_check,
    };
}

/// Helper functions for policy evaluation in custom rules.
pub mod helpers {
    use crate::actor::Actor;

    pub fn is_owner(viewer: &Actor, record_id: &str) -> bool {
        if let Some(viewer_id) = viewer.user_id() {
            viewer_id == record_id
        } else {
            false
        }
    }

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
