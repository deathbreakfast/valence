//! `composite_id`, `get_by_composite_key`, and `upsert_by_composite_key` for multi-column keys.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;

/// Borrowed parameter type for composite_id function parameters.
fn composite_id_param_type(field_type_str: &str, is_required: bool) -> TokenStream {
    let base = if field_type_str.starts_with("record<") && field_type_str.ends_with('>') {
        quote! { &valence::RecordId }
    } else {
        match field_type_str {
            "string" => quote! { &str },
            "integer" => quote! { i64 },
            "float" => quote! { f64 },
            "boolean" => quote! { bool },
            "datetime" => quote! { &chrono::DateTime<chrono::Utc> },
            "json" => quote! { &serde_json::Value },
            _ => quote! { &str },
        }
    };

    if is_required {
        base
    } else {
        quote! { Option<#base> }
    }
}

/// Expression to convert a composite_id parameter into its String component.
fn composite_id_component_expr(
    param_ident: &proc_macro2::Ident,
    field_type_str: &str,
    is_required: bool,
) -> TokenStream {
    let is_string = field_type_str == "string";
    let is_record = field_type_str.starts_with("record<") && field_type_str.ends_with('>');

    if is_required {
        if is_record {
            quote! { #param_ident.id().to_string() }
        } else if is_string {
            quote! { #param_ident.to_string() }
        } else {
            quote! { #param_ident.to_string() }
        }
    } else if is_string {
        quote! { #param_ident.unwrap_or("__null__").to_string() }
    } else if is_record {
        quote! {
            #param_ident.map(|r| r.id().to_string()).unwrap_or_else(|| "__null__".to_string())
        }
    } else {
        quote! {
            #param_ident.map(|v| v.to_string()).unwrap_or_else(|| "__null__".to_string())
        }
    }
}

/// Expression to extract a composite key field value from a model instance for
/// passing to `composite_id()`.
fn composite_id_accessor_expr(
    field_ident: &proc_macro2::Ident,
    field_type_str: &str,
    is_required: bool,
) -> TokenStream {
    let is_string = field_type_str == "string";
    let is_copy = matches!(field_type_str, "integer" | "float" | "boolean");
    let is_record = field_type_str.starts_with("record<") && field_type_str.ends_with('>');

    if is_required {
        if is_copy {
            quote! { *data.#field_ident() }
        } else if is_record {
            quote! { data.#field_ident() }
        } else {
            quote! { data.#field_ident() }
        }
    } else if is_string {
        quote! { data.#field_ident().map(|s| s.as_str()) }
    } else if is_copy {
        quote! { data.#field_ident().copied() }
    } else {
        quote! { data.#field_ident() }
    }
}

pub(super) fn generate_composite_key_methods(
    schema: &SchemaContext,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    if schema.composite_key.is_empty() {
        return Ok(TokenStream::new());
    }

    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));

    struct CkField {
        ident: proc_macro2::Ident,
        field_type_str: String,
        is_required: bool,
    }

    let ck_fields: Vec<CkField> = schema
        .composite_key
        .iter()
        .map(|name| {
            let field = schema
                .fields
                .iter()
                .find(|f| f.name == *name)
                .unwrap_or_else(|| panic!("composite_key field '{name}' not found in schema"));
            CkField {
                ident: format_ident!("{}", name),
                field_type_str: field.field_type.clone(),
                is_required: !field.nullable,
            }
        })
        .collect();

    let params: Vec<TokenStream> = ck_fields
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let ty = composite_id_param_type(&f.field_type_str, f.is_required);
            quote! { #ident: #ty }
        })
        .collect();

    let components: Vec<TokenStream> = ck_fields
        .iter()
        .map(|f| composite_id_component_expr(&f.ident, &f.field_type_str, f.is_required))
        .collect();

    let forward_args: Vec<TokenStream> = ck_fields
        .iter()
        .map(|f| {
            let ident = &f.ident;
            quote! { #ident }
        })
        .collect();

    let accessor_args: Vec<TokenStream> = ck_fields
        .iter()
        .map(|f| composite_id_accessor_expr(&f.ident, &f.field_type_str, f.is_required))
        .collect();

    Ok(quote! {
        impl #struct_name {
            /// Build the composite primary key string from the component fields.
            pub fn composite_id(#(#params),*) -> String {
                [#(#components),*].join(":")
            }

            /// Point-lookup by composite key fields.
            pub async fn get_by_composite_key(
                valence: &valence::Valence,
                #(#params),*
            ) -> valence::Result<Option<Self>> {
                let id = Self::composite_id(#(#forward_args),*);
                #[allow(deprecated)]
                <Self as valence::Model>::get(&id, valence).await
            }

            /// Upsert using the composite key derived from the model's fields.
            pub async fn upsert_by_composite_key(
                data: Self,
                valence: &valence::Valence,
            ) -> valence::Result<Self> {
                let id = Self::composite_id(#(#accessor_args),*);
                #[allow(deprecated)]
                <Self as valence::Model>::upsert(&id, data, valence).await
            }
        }
    })
}
