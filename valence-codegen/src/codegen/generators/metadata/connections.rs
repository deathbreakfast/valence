//! Connection literal emission for schema metadata.

use proc_macro2::TokenStream;
use quote::quote;
use syn::LitStr;

use valence_core::SchemaConnection;

use super::string_helpers::optional_string_lit_code;

/// One `SchemaConnection { ... }` literal for `full()` metadata.
pub(super) fn schema_connection_tokens(conn: &SchemaConnection) -> TokenStream {
    let name_lit = LitStr::new(&conn.name, proc_macro2::Span::call_site());
    let from_table_lit = LitStr::new(&conn.from_table, proc_macro2::Span::call_site());
    let from_field_lit = LitStr::new(&conn.from_field, proc_macro2::Span::call_site());
    let to_table_lit = LitStr::new(&conn.to_table, proc_macro2::Span::call_site());
    let cardinality_lit = LitStr::new(&conn.cardinality, proc_macro2::Span::call_site());
    let on_delete_lit = LitStr::new(&conn.on_delete, proc_macro2::Span::call_site());
    let label_lit = LitStr::new(&conn.label, proc_macro2::Span::call_site());
    let required = conn.required;
    let model_path_code = optional_string_lit_code(&conn.model_path);
    let reverse_field_code = optional_string_lit_code(&conn.reverse_field);
    let edge_table_code = optional_string_lit_code(&conn.edge_table);
    let edge_policy_code = optional_string_lit_code(&conn.edge_policy);
    let target_trait_code = optional_string_lit_code(&conn.target_trait);

    quote! {
        valence::SchemaConnection {
            name: #name_lit.to_string(),
            from_table: #from_table_lit.to_string(),
            from_field: #from_field_lit.to_string(),
            to_table: #to_table_lit.to_string(),
            cardinality: #cardinality_lit.to_string(),
            required: #required,
            on_delete: #on_delete_lit.to_string(),
            label: #label_lit.to_string(),
            model_path: #model_path_code,
            reverse_field: #reverse_field_code,
            edge_table: #edge_table_code,
            edge_policy: #edge_policy_code,
            target_trait: #target_trait_code,
        }
    }
}
