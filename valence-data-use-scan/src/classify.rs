//! Method → op and receiver → target classification.

use crate::{OpKind, TargetKind};

/// Map a `*_used` method name to a UI op bucket.
#[must_use]
pub fn classify_method(method: &str) -> OpKind {
    let base = method.strip_suffix("_used").unwrap_or(method);
    match base {
        "create" => OpKind::Create,
        "update" | "merge" | "upsert" | "upsert_by_composite_key" | "commit" => OpKind::Update,
        "delete" | "delete_now" => OpKind::Delete,
        // get, query, get_mutable, execute, get_entity, get_record*, latest_ids, …
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
    if receiver.ends_with("QueryAll") || (method == "query_used" && receiver.contains("QueryAll")) {
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
        assert_eq!(classify_method("get_used"), OpKind::Read);
        assert_eq!(classify_method("query_used"), OpKind::Read);
        assert_eq!(classify_method("get_mutable_used"), OpKind::Read);
        assert_eq!(classify_method("execute_used"), OpKind::Read);
        assert_eq!(classify_method("create_used"), OpKind::Create);
        assert_eq!(classify_method("merge_used"), OpKind::Update);
        assert_eq!(classify_method("upsert_used"), OpKind::Update);
        assert_eq!(classify_method("delete_now_used"), OpKind::Delete);
    }

    #[test]
    fn targets() {
        assert_eq!(
            classify_target("User", "get_used"),
            TargetKind::Schema("user".into())
        );
        assert_eq!(
            classify_target("NamedQueryAll", "query_used"),
            TargetKind::Trait("Named".into())
        );
        assert_eq!(
            classify_target("QueryCore", "execute_used"),
            TargetKind::Unscoped
        );
        assert_eq!(
            classify_target("crate::models::UserSession", "get_used"),
            TargetKind::Schema("user_session".into())
        );
    }
}
