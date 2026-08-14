//! Shared Rust type mapping for generated model fields.
//!
//! Keeps struct / CRUD / FieldChange / trait generators aligned on `json_as:`,
//! `currency`, `datetime`, and related wire types.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse_str;
use valence_core::SchemaField;

use crate::codegen::utils::to_pascal_case;

/// Parsed `json_as:path` / `json_as:path;serde_error=panic` wire form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonAsMeta {
    pub type_path: String,
    pub panic_on_error: bool,
}

/// Parse JsonAs wire type; `None` if not a `json_as:` field.
pub fn parse_json_as(field_type: &str) -> Option<JsonAsMeta> {
    let rest = field_type.strip_prefix("json_as:")?;
    let (type_path, panic_on_error) = if let Some((path, policy)) = rest.split_once(";serde_error=")
    {
        (path.to_string(), policy == "panic")
    } else {
        (rest.to_string(), false)
    };
    Some(JsonAsMeta {
        type_path,
        panic_on_error,
    })
}

/// Rust type tokens for a schema field (excluding primary-key special casing).
pub fn rust_type_tokens(field: &SchemaField, model_name: &str) -> TokenStream {
    let ft = field.field_type.as_str();

    if let Some(meta) = parse_json_as(ft) {
        return parse_str::<syn::Type>(&meta.type_path)
            .map(|ty| quote! { #ty })
            .unwrap_or_else(|_| {
                let ident = format_ident!("{}", meta.type_path.replace("::", "_"));
                quote! { #ident }
            });
    }

    if ft.starts_with("enum:") || ft.starts_with("ext_enum:") {
        if let Some(ref etype) = field.enum_type {
            return etype.parse().unwrap_or_else(|_| {
                let ident = format_ident!("{}", etype);
                quote! { #ident }
            });
        }
        if ft.starts_with("enum:") {
            let enum_name = format!("{}{}", model_name, to_pascal_case(&field.name));
            let ident = format_ident!("{}", enum_name);
            return quote! { #ident };
        }
    }

    if ft.starts_with("record<") && ft.ends_with('>') {
        return quote! { valence::RecordId };
    }

    match ft {
        "string" => quote! { String },
        "integer" => quote! { i64 },
        "float" => quote! { f64 },
        "boolean" => quote! { bool },
        "datetime" => quote! { chrono::DateTime<chrono::Utc> },
        "json" => quote! { serde_json::Value },
        "currency" => quote! { valence::Currency },
        _ => quote! { String },
    }
}

/// Whether the field’s Rust type is `Copy` (prefer move/`*` over `.clone()` in generated code).
#[must_use]
pub fn field_type_is_copy(field: &SchemaField) -> bool {
    matches!(
        field.field_type.as_str(),
        "integer" | "float" | "boolean" | "datetime" | "currency"
    )
}

fn sanitize_ident(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Helper function tokens + field attribute for a JsonAs field.
pub fn json_as_helpers_and_attrs(
    field: &SchemaField,
    table_name: &str,
    rust_ty: &TokenStream,
) -> Option<(TokenStream, TokenStream)> {
    let meta = parse_json_as(&field.field_type)?;
    let type_path = &meta.type_path;
    let field_name = field.name.as_str();
    let ser_fn = format_ident!(
        "__valence_json_as_ser_{}_{}",
        sanitize_ident(table_name),
        sanitize_ident(field_name)
    );
    let de_fn = format_ident!(
        "__valence_json_as_de_{}_{}",
        sanitize_ident(table_name),
        sanitize_ident(field_name)
    );
    let mode = if meta.panic_on_error {
        quote! { valence::JsonAsSerdeError::Panic }
    } else {
        quote! { valence::JsonAsSerdeError::Error }
    };
    let table_lit = table_name;
    let field_lit = field_name;

    let helpers = quote! {
        #[allow(non_snake_case, dead_code)]
        fn #ser_fn<S>(
            value: &#rust_ty,
            serializer: S,
        ) -> core::result::Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            valence::json_as::serialize(
                value,
                serializer,
                #table_lit,
                #field_lit,
                #type_path,
                #mode,
            )
        }

        #[allow(non_snake_case, dead_code)]
        fn #de_fn<'de, D>(
            deserializer: D,
        ) -> core::result::Result<#rust_ty, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            valence::json_as::deserialize(
                deserializer,
                #table_lit,
                #field_lit,
                #type_path,
                #mode,
            )
        }
    };

    let ser_name = ser_fn.to_string();
    let de_name = de_fn.to_string();
    let attrs = if field.nullable {
        // Option<T> needs option adapters — deserialize Option via default + inner.
        // For v0.1 required JsonAs is the primary path; optional uses default None
        // without custom with (plain Option<T> Deserialize).
        quote! {
            #[serde(default)]
        }
    } else {
        quote! {
            #[serde(serialize_with = #ser_name, deserialize_with = #de_name)]
        }
    };

    Some((helpers, attrs))
}
