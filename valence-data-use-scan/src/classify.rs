//! Method → op and receiver → target classification.

use crate::{ConnectionHop, ConnectionHopKind, OpKind, TargetKind};

/// Map a declared data-use method name to a UI op bucket.
#[must_use]
pub fn classify_method(method: &str) -> OpKind {
    let base = method;
    match base {
        "create" => OpKind::Create,
        "update" | "merge" | "upsert" | "upsert_by_composite_key" | "commit" => OpKind::Update,
        "delete" | "delete_now" => OpKind::Delete,
        // M2M / Valence edge mutates
        b if b.starts_with("relate_to")
            || b.starts_with("unrelate_from")
            || b.starts_with("relate_edge")
            || b.starts_with("unrelate_edge") =>
        {
            OpKind::Update
        }
        // get, query, get_mutable, execute, get_entity, get_record*, latest_ids,
        // get_user / get_from_* / get_*_record_ids, …
        _ => OpKind::Read,
    }
}

/// Map a receiver type / path to Schema / Trait / Unscoped.
#[must_use]
pub fn classify_target(receiver: &str, method: &str) -> TargetKind {
    let receiver = receiver.trim();
    if receiver.is_empty() || is_unscoped_receiver(receiver) {
        return TargetKind::Unscoped;
    }
    if receiver.ends_with("QueryAll") || (method == "query" && receiver.contains("QueryAll")) {
        let name = receiver
            .rsplit("::")
            .next()
            .unwrap_or(receiver)
            .strip_suffix("QueryAll")
            .unwrap_or(receiver);
        return TargetKind::Trait(name.to_string());
    }
    let type_name = receiver.rsplit("::").next().unwrap_or(receiver);
    // Mutable helpers still belong to the schema type prefix (`UserMutable`).
    let type_name = type_name.strip_suffix("Mutable").unwrap_or(type_name);
    if type_name.ends_with("Query") {
        let base = type_name.strip_suffix("Query").unwrap_or(type_name);
        return TargetKind::Schema(pascal_to_snake(base));
    }
    TargetKind::Schema(pascal_to_snake(type_name))
}

fn is_unscoped_receiver(receiver: &str) -> bool {
    let leaf = receiver.rsplit("::").next().unwrap_or(receiver);
    matches!(
        leaf,
        "QueryCore" | "DatabaseBackend" | "DynDatabaseBackend" | "Backend" | "Valence" | "Self"
    )
}

/// Detect a forward connection load or edge mutate from a declared method name.
///
/// Reverse helpers (`get_from_*`) return [`None`] — they load the receiver schema,
/// not the peer. Unscoped `relate_edge` / `unrelate_edge` also return [`None`].
#[must_use]
pub fn classify_connection_hop(method: &str) -> Option<ConnectionHop> {
    let base = method;

    // Reverse navigators load the initiating schema — not peer-referenced.
    if base.starts_with("get_from_") {
        return None;
    }

    if let Some(rest) = base.strip_prefix("relate_to_") {
        return relate_field(rest).map(|field| ConnectionHop {
            field,
            kind: ConnectionHopKind::Relate,
        });
    }
    if let Some(rest) = base.strip_prefix("unrelate_from_") {
        return relate_field(rest).map(|field| ConnectionHop {
            field,
            kind: ConnectionHopKind::Relate,
        });
    }

    // Forward connection gets: get_{field} / get_{field}_record_ids.
    // Exclude bare get / get_mutable / get_entity / get_record*.
    let rest = base.strip_prefix("get_")?;
    if rest.is_empty()
        || rest == "mutable"
        || rest.starts_with("entity")
        || rest == "record"
        || rest.starts_with("record_")
    {
        return None;
    }
    let field = rest.strip_suffix("_record_ids").unwrap_or(rest).to_string();
    if field.is_empty() {
        return None;
    }
    Some(ConnectionHop {
        field,
        kind: ConnectionHopKind::ForwardGet,
    })
}

fn relate_field(rest: &str) -> Option<String> {
    // `relate_edge` / `unrelate_edge` land here only if prefixed wrongly; guard anyway.
    if rest.is_empty() || rest == "edge" || rest.starts_with("edge_") {
        return None;
    }
    let field = rest.strip_suffix("_record").unwrap_or(rest);
    if field.is_empty() {
        return None;
    }
    Some(field.to_string())
}

/// Match a hop field to a schema connection `from_field` (exact or M2M singular).
#[must_use]
pub fn hop_field_matches_connection(hop_field: &str, from_field: &str) -> bool {
    if hop_field == from_field {
        return true;
    }
    // M2M relate_to_{singular} vs connection name often plural (`tags` / `tag`).
    let singular = from_field.strip_suffix('s').unwrap_or(from_field);
    hop_field == singular
}

/// Convert `UserSession` → `user_session`.
#[must_use]
pub fn pascal_to_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_ops() {
        assert_eq!(classify_method("get"), OpKind::Read);
        assert_eq!(classify_method("query"), OpKind::Read);
        assert_eq!(classify_method("get_mutable"), OpKind::Read);
        assert_eq!(classify_method("execute"), OpKind::Read);
        assert_eq!(classify_method("get_user"), OpKind::Read);
        assert_eq!(classify_method("get_from_user_id"), OpKind::Read);
        assert_eq!(classify_method("get_owners_record_ids"), OpKind::Read);
        assert_eq!(classify_method("create"), OpKind::Create);
        assert_eq!(classify_method("merge"), OpKind::Update);
        assert_eq!(classify_method("upsert"), OpKind::Update);
        assert_eq!(classify_method("delete_now"), OpKind::Delete);
        assert_eq!(classify_method("relate_to_owner_record"), OpKind::Update);
        assert_eq!(classify_method("unrelate_from_tag"), OpKind::Update);
        assert_eq!(classify_method("relate_edge"), OpKind::Update);
        assert_eq!(classify_method("unrelate_edge"), OpKind::Update);
    }

    #[test]
    fn targets() {
        assert_eq!(
            classify_target("User", "get"),
            TargetKind::Schema("user".into())
        );
        assert_eq!(
            classify_target("NamedQueryAll", "query"),
            TargetKind::Trait("Named".into())
        );
        assert_eq!(
            classify_target("QueryCore", "execute"),
            TargetKind::Unscoped
        );
        assert_eq!(
            classify_target("crate::models::UserSession", "get"),
            TargetKind::Schema("user_session".into())
        );
    }

    #[test]
    fn hop_forward_get_owner() {
        let hop = classify_connection_hop("get_owner").expect("hop");
        assert_eq!(hop.kind, ConnectionHopKind::ForwardGet);
        assert_eq!(hop.field, "owner");
    }

    #[test]
    fn hop_forward_get_record_ids() {
        let hop = classify_connection_hop("get_tags_record_ids").expect("hop");
        assert_eq!(hop.kind, ConnectionHopKind::ForwardGet);
        assert_eq!(hop.field, "tags");
    }

    #[test]
    fn hop_relate_and_unrelate() {
        let relate = classify_connection_hop("relate_to_tag").expect("relate");
        assert_eq!(relate.kind, ConnectionHopKind::Relate);
        assert_eq!(relate.field, "tag");
        let unrelate = classify_connection_hop("unrelate_from_tag").expect("unrelate");
        assert_eq!(unrelate.kind, ConnectionHopKind::Relate);
        assert_eq!(unrelate.field, "tag");
        let record = classify_connection_hop("relate_to_owner_record").expect("record");
        assert_eq!(record.field, "owner");
    }

    #[test]
    fn hop_excludes_plain_get_and_reverse() {
        assert!(classify_connection_hop("get").is_none());
        assert!(classify_connection_hop("get_mutable").is_none());
        assert!(classify_connection_hop("get_from_owner_id").is_none());
        assert!(classify_connection_hop("get_from_owner").is_none());
        assert!(classify_connection_hop("relate_edge").is_none());
        assert!(classify_connection_hop("unrelate_edge").is_none());
        assert!(classify_connection_hop("update").is_none());
        assert!(classify_connection_hop("query").is_none());
    }

    #[test]
    fn hop_field_matches_plural_connection() {
        assert!(hop_field_matches_connection("tags", "tags"));
        assert!(hop_field_matches_connection("tag", "tags"));
        assert!(!hop_field_matches_connection("owner", "tags"));
    }
}
