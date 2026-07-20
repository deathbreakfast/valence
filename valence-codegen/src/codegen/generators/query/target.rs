//! Resolve target model query types for connection hops (`model:` path → `*Query`).

use syn::parse_str;

use crate::codegen::utils::to_pascal_case;

/// Build the fully-qualified query builder path string for a connection target.
pub(super) fn resolve_target_query_path(conn: &valence_core::SchemaConnection) -> String {
    let target_table_pascal = to_pascal_case(&conn.to_table);
    let model_path = conn
        .model_path
        .clone()
        .unwrap_or_else(|| format!("crate::generated::{target_table_pascal}"));
    format!("{model_path}Query")
}

/// Resolve the target query builder type (without lifetime) for struct init expressions.
pub(super) fn resolve_target_query_type(conn: &valence_core::SchemaConnection) -> syn::Type {
    let query_path = resolve_target_query_path(conn);
    parse_str(&query_path).unwrap_or_else(|_| {
        #[allow(clippy::expect_used)] // constant fallback path
        {
            parse_str("crate::generated::UnknownQuery").expect("fallback query type")
        }
    })
}

/// Resolve the target query builder type with `<'a>` lifetime for return type annotations.
pub(super) fn resolve_target_query_type_with_lifetime(
    conn: &valence_core::SchemaConnection,
) -> syn::Type {
    let query_path = resolve_target_query_path(conn);
    let with_lt = format!("{query_path}<'a>");
    parse_str(&with_lt).unwrap_or_else(|_| {
        #[allow(clippy::expect_used)] // constant fallback path
        {
            parse_str("crate::generated::UnknownQuery<'a>").expect("fallback query type")
        }
    })
}

/// Whether the connection target is in the same crate (typed hops only for `crate::` paths).
pub(super) fn is_same_crate_connection(conn: &valence_core::SchemaConnection) -> bool {
    match &conn.model_path {
        Some(path) => path.starts_with("crate::"),
        None => true,
    }
}
