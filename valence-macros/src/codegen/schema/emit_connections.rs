//! Connection token emission for [`super`] schema expansion.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::LitStr;

use valence_schema_dsl;

use super::emit_fields::record_table;

pub(super) fn connections_tokens(
    fields: &[valence_schema_dsl::ParsedField],
    parsed_connections: &[valence_schema_dsl::ParsedConnection],
    from_table: &str,
) -> Vec<TokenStream2> {
    if !parsed_connections.is_empty() {
        return parsed_connections
            .iter()
            .map(|conn| parsed_connection_tokens(conn, from_table))
            .collect();
    }

    fields
        .iter()
        .filter_map(|field| {
            let ref_table = record_table(&field.field_type)?;
            let field_name_lit = LitStr::new(&field.name, proc_macro2::Span::call_site());
            let from_table_lit = LitStr::new(from_table, proc_macro2::Span::call_site());
            let ref_table_lit = LitStr::new(ref_table, proc_macro2::Span::call_site());
            let label = edge_label(&field.name);
            let label_lit = LitStr::new(&label, proc_macro2::Span::call_site());

            let model_path_code = optional_string_lit(field.model_path.as_ref());

            Some(quote! {
                valence::SchemaConnection {
                    name: #field_name_lit.to_string(),
                    from_table: #from_table_lit.to_string(),
                    from_field: #field_name_lit.to_string(),
                    to_table: #ref_table_lit.to_string(),
                    cardinality: "HasOne".to_string(),
                    required: true,
                    on_delete: "Cascade".to_string(),
                    label: #label_lit.to_string(),
                    model_path: #model_path_code,
                    reverse_field: None,
                    edge_table: None,
                    edge_policy: None,
                    target_trait: None,
                }
            })
        })
        .collect()
}

fn optional_string_lit(opt: Option<&String>) -> TokenStream2 {
    if let Some(v) = opt {
        let lit = LitStr::new(v, proc_macro2::Span::call_site());
        quote! { Some(#lit.to_string()) }
    } else {
        quote! { None }
    }
}

fn parsed_connection_tokens(
    conn: &valence_schema_dsl::ParsedConnection,
    from_table: &str,
) -> TokenStream2 {
    let name_lit = LitStr::new(&conn.name, proc_macro2::Span::call_site());
    let from_table_lit = LitStr::new(from_table, proc_macro2::Span::call_site());
    let to_table_lit = LitStr::new(&conn.table, proc_macro2::Span::call_site());
    let cardinality_lit = LitStr::new(&conn.cardinality, proc_macro2::Span::call_site());
    let on_delete_lit = LitStr::new(&conn.on_delete, proc_macro2::Span::call_site());
    let label = edge_label(&conn.name);
    let label_lit = LitStr::new(&label, proc_macro2::Span::call_site());
    let required = conn.required;

    let model_code = optional_string_lit(conn.model.as_ref());
    let reverse_field_code = optional_string_lit(conn.reverse_field.as_ref());
    let edge_table_code = optional_string_lit(conn.edge_table.as_ref());
    let edge_policy_code = optional_string_lit(conn.edge_policy.as_ref());
    let target_trait_code = optional_string_lit(conn.target_trait.as_ref());

    quote! {
        valence::SchemaConnection {
            name: #name_lit.to_string(),
            from_table: #from_table_lit.to_string(),
            from_field: #name_lit.to_string(),
            to_table: #to_table_lit.to_string(),
            cardinality: #cardinality_lit.to_string(),
            required: #required,
            on_delete: #on_delete_lit.to_string(),
            label: #label_lit.to_string(),
            model_path: #model_code,
            reverse_field: #reverse_field_code,
            edge_table: #edge_table_code,
            edge_policy: #edge_policy_code,
            target_trait: #target_trait_code,
        }
    }
}

pub(super) fn edge_label(field_name: &str) -> String {
    field_name
        .strip_suffix("_id")
        .unwrap_or(field_name)
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
