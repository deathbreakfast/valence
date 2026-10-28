//! Internal predicate functions backing reusable privacy rules.

use crate::actor::Actor;
use serde_json::Value as JsonValue;

pub(super) fn public_read_check(_record: &JsonValue, _viewer: &Actor) -> bool {
    true
}

pub(super) fn authenticated_check(_record: &JsonValue, viewer: &Actor) -> bool {
    viewer.is_user() || viewer.is_system() || viewer.is_service_user()
}

pub(super) fn system_only_check(_record: &JsonValue, viewer: &Actor) -> bool {
    viewer.is_system()
}

pub(super) fn block_all_check(_record: &JsonValue, _viewer: &Actor) -> bool {
    true
}

pub(super) fn owner_by_id_check(record: &JsonValue, viewer: &Actor) -> bool {
    super::definitions::owner::check_owner_by_field(record, viewer, "id")
}

pub(super) fn owner_by_user_field_check(record: &JsonValue, viewer: &Actor) -> bool {
    super::definitions::owner::check_owner_by_field(record, viewer, "user")
}
