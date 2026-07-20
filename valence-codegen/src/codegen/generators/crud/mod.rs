//! CRUD [`valence::Model`] impl, unique constraints, privacy checks, and mutable builder.

mod composite_key;
mod emit;
mod emit_create;
mod emit_ctx;
mod emit_delete;
mod emit_get;
mod emit_model_ops;
mod emit_ownership;
mod emit_update;
mod field_tokens;
mod mutable_parts;

use proc_macro2::TokenStream;
use quote::format_ident;
use syn::LitStr;

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;

use self::composite_key::generate_composite_key_methods;
use self::emit::emit_crud_tokens;
use self::emit_ctx::CrudEmitCtx;
use self::mutable_parts::collect_mutable_part_streams;

/// Generate `Model` trait impl, mutable builder, and composite-key helpers for one schema.
pub fn generate_crud_operations(
    schema: &SchemaContext,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let schema_struct_name = format_ident!("{}Schema", struct_name);
    let mutable_name = format_ident!("{}Mutable", struct_name);
    let table_name_lit = schema.table_name.as_str();

    let parts = collect_mutable_part_streams(schema);

    let version_lit = schema.schema.version.as_str();
    let field_changes_name = format_ident!("{}FieldChanges", struct_name);
    let unique_field_names: Vec<LitStr> = schema
        .fields
        .iter()
        .filter(|field| field.unique && !field.primary)
        .map(|field| LitStr::new(field.name.as_str(), proc_macro2::Span::call_site()))
        .collect();

    let composite_key_methods = generate_composite_key_methods(schema)?;

    let table = schema.table_name.as_str();
    let ownership_skip = matches!(
        table,
        "valence_data_ownership" | "valence_ownership_transfer"
    );
    let deletion_skip = matches!(
        table,
        "valence_data_ownership"
            | "valence_ownership_transfer"
            | "valence_deletion_run"
            | "valence_deletion_step"
            | "valence_deletion_error"
            | "valence_iter_run"
            | "valence_iter_batch"
            | "valence_iter_row_error"
    );
    let ownership_system_owned = schema
        .schema
        .ownership
        .as_ref()
        .is_some_and(|o| o.system_owned);
    let ownership_resolver = schema
        .schema
        .ownership
        .as_ref()
        .and_then(|o| o.resolve.as_deref())
        .map(syn::parse_str::<syn::Path>)
        .transpose()
        .map_err(|e| {
            format!(
                "invalid ownership.resolve path for table {}: {}",
                schema.table_name, e
            )
        })?;

    Ok(emit_crud_tokens(&CrudEmitCtx {
        struct_name,
        schema_struct_name,
        mutable_name,
        table_name_lit,
        version_lit,
        field_changes_name,
        unique_field_names,
        parts: &parts,
        composite_key_methods,
        ownership_skip,
        deletion_skip,
        ownership_system_owned,
        ownership_resolver,
    }))
}
