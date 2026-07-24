//! Field and edge literal emission for schema metadata.

use proc_macro2::TokenStream;
use quote::quote;
use syn::LitStr;

use crate::codegen::utils::map_field_type_to_string;
use valence_core::SchemaField;
use valence_schema_dsl::ParsedPolicies;

use super::policies::generate_policies_code;
use super::string_helpers::humanize_field_edge_label;

/// Optional `SchemaEdge` for FK-backed fields; plus the `SchemaField` row for `full()`.
pub(super) fn schema_field_and_edge_tokens(
    field: &SchemaField,
    field_policies: Option<&ParsedPolicies>,
) -> (Option<TokenStream>, TokenStream) {
    let field_name_str = field.name.as_str();
    let field_type_str = field.field_type.as_str();
    let field_type_display = map_field_type_to_string(field_type_str);
    let field_type_lit = LitStr::new(&field_type_display, proc_macro2::Span::call_site());

    let is_primary = field.primary;
    let nullable = field.nullable;
    let default_lit = field
        .default
        .as_ref()
        .map(|d| LitStr::new(d, proc_macro2::Span::call_site()));

    let fk_ref = field
        .fk
        .as_ref()
        .map(|fk| (fk.ref_table.clone(), fk.field.clone()));

    let fk_ref_code = if let Some((ref_table, ref_field)) = &fk_ref {
        let ref_table_lit = LitStr::new(ref_table, proc_macro2::Span::call_site());
        let ref_field_lit = LitStr::new(ref_field, proc_macro2::Span::call_site());
        quote! {
            Some(valence::ForeignKeyRef {
                ref_table: #ref_table_lit.to_string(),
                field: #ref_field_lit.to_string(),
            })
        }
    } else {
        quote! { None }
    };

    let edge_ts = if let Some((ref_table, _)) = &fk_ref {
        let field_name_lit = LitStr::new(field_name_str, proc_macro2::Span::call_site());
        let ref_table_lit = LitStr::new(ref_table, proc_macro2::Span::call_site());
        let label = humanize_field_edge_label(field_name_str);
        let label_lit = LitStr::new(&label, proc_macro2::Span::call_site());
        Some(quote! {
            valence::SchemaEdge {
                from_field: #field_name_lit.to_string(),
                to_table: #ref_table_lit.to_string(),
                label: #label_lit.to_string(),
            }
        })
    } else {
        None
    };

    let field_name_lit = LitStr::new(field_name_str, proc_macro2::Span::call_site());

    let default_code = if let Some(d) = &default_lit {
        quote! { Some(#d.to_string()) }
    } else {
        quote! { None }
    };

    let validations_code = if field.validations.is_empty() {
        quote! { Vec::new() }
    } else {
        let vals: Vec<LitStr> = field
            .validations
            .iter()
            .map(|v| LitStr::new(v, proc_macro2::Span::call_site()))
            .collect();
        quote! { vec![#(#vals.to_string()),*] }
    };

    let encrypted = field.encrypted;
    let unique = field.unique;
    let policies_code = generate_policies_code(field_policies);

    let model_path_code = if let Some(ref path) = field.model_path {
        let lit = LitStr::new(path, proc_macro2::Span::call_site());
        quote! { Some(#lit.to_string()) }
    } else {
        quote! { None }
    };

    let row = quote! {
        valence::SchemaField {
            name: #field_name_lit.to_string(),
            field_type: #field_type_lit.to_string(),
            primary: #is_primary,
            nullable: #nullable,
            indexed: false,
            unique: #unique,
            default: #default_code,
            fk: #fk_ref_code,
            validations: #validations_code,
            policies: #policies_code,
            encrypted: #encrypted,
            enum_variants: Vec::new(),
            enum_type: None,
            model_path: #model_path_code,
        }
    };

    (edge_ts, row)
}
