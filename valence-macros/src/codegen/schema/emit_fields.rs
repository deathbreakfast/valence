//! Field and edge token emission for [`super`] schema expansion.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::LitStr;

use crate::codegen::policies;
use valence_schema_dsl;

pub(super) fn fields_tokens(fields: &[valence_schema_dsl::ParsedField]) -> Vec<TokenStream2> {
    fields
        .iter()
        .map(|field| {
            let name_lit = LitStr::new(&field.name, proc_macro2::Span::call_site());
            let field_type_lit = LitStr::new(&field.field_type, proc_macro2::Span::call_site());
            let default_code = field.default.as_ref().map_or_else(
                || quote! { None },
                |d| {
                    let lit = LitStr::new(d, proc_macro2::Span::call_site());
                    quote! { Some(#lit.to_string()) }
                },
            );
            let validations_code = super::string_vec_tokens(&field.validations);
            let policies_code = policies::policies_tokens(field.policies.as_ref());
            let primary_key = field.primary_key;
            let unique = field.unique;
            let indexed = field.indexed;
            let encrypted = field.encrypted;
            let nullable = !field.required;

            let fk_code = if let Some(ref_table) = record_table(&field.field_type) {
                let ref_table_lit = LitStr::new(ref_table, proc_macro2::Span::call_site());
                quote! {
                    Some(valence::ForeignKeyRef {
                        ref_table: #ref_table_lit.to_string(),
                        field: "id".to_string(),
                    })
                }
            } else {
                quote! { None }
            };

            let model_path_code = field.model_path.as_ref().map_or_else(
                || quote! { None },
                |p| {
                    let lit = LitStr::new(p, proc_macro2::Span::call_site());
                    quote! { Some(#lit.to_string()) }
                },
            );

            quote! {
                valence::SchemaField {
                    name: #name_lit.to_string(),
                    field_type: #field_type_lit.to_string(),
                    primary: #primary_key,
                    nullable: #nullable,
                    indexed: #indexed,
                    unique: #unique,
                    default: #default_code,
                    fk: #fk_code,
                    validations: #validations_code,
                    policies: #policies_code,
                    encrypted: #encrypted,
                    enum_variants: Vec::new(),
                    enum_type: None,
                    model_path: #model_path_code,
                }
            }
        })
        .collect()
}

pub(super) fn edges_tokens(fields: &[valence_schema_dsl::ParsedField]) -> Vec<TokenStream2> {
    fields
        .iter()
        .filter_map(|field| {
            let ref_table = record_table(&field.field_type)?;
            let field_name_lit = LitStr::new(&field.name, proc_macro2::Span::call_site());
            let ref_table_lit = LitStr::new(ref_table, proc_macro2::Span::call_site());
            let label = super::emit_connections::edge_label(&field.name);
            let label_lit = LitStr::new(&label, proc_macro2::Span::call_site());

            Some(quote! {
                valence::SchemaEdge {
                    from_field: #field_name_lit.to_string(),
                    to_table: #ref_table_lit.to_string(),
                    label: #label_lit.to_string(),
                }
            })
        })
        .collect()
}

pub(super) fn record_table(field_type: &str) -> Option<&str> {
    if field_type.starts_with("record<") && field_type.ends_with('>') {
        return field_type
            .strip_prefix("record<")
            .and_then(|s| s.strip_suffix('>'));
    }
    None
}
