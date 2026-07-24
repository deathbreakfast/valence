//! Collect schema metadata token fragments before quoting.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::LitStr;

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;

use super::connections::schema_connection_tokens;
use super::fields::schema_field_and_edge_tokens;
use super::policies::generate_policies_code;
use super::string_helpers::string_names_to_vec_code;

/// Collected literals and token fragments for `generate_schema_metadata_method`.
pub(super) struct SchemaMetadataPieces {
    pub struct_name: proc_macro2::Ident,
    pub schema_struct_name: proc_macro2::Ident,
    pub version_lit: LitStr,
    pub read_lit: LitStr,
    pub write_lit: LitStr,
    pub table_name_lit: LitStr,
    pub schema_fields: Vec<TokenStream>,
    pub edges: Vec<TokenStream>,
    pub connections: Vec<TokenStream>,
    pub description_code: TokenStream,
    pub description_const_code: TokenStream,
    pub policies_code: TokenStream,
    pub trait_names_code: TokenStream,
    pub side_effects_code: TokenStream,
    pub iters_code: TokenStream,
    pub ttl_code: TokenStream,
    pub composite_key_code: TokenStream,
    pub ownership_code: TokenStream,
    pub database_evaluator_code: TokenStream,
    pub database_typecheck_code: TokenStream,
}

pub(super) fn collect_schema_metadata_pieces(schema: &SchemaContext) -> SchemaMetadataPieces {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let schema_struct_name = format_ident!("{}Schema", struct_name);

    let version_lit = LitStr::new(&schema.schema.version, proc_macro2::Span::call_site());
    let description_lit = schema
        .schema
        .meta
        .description
        .as_ref()
        .map(|d| LitStr::new(d, proc_macro2::Span::call_site()));

    let read_lit = LitStr::new(&schema.schema.privacy.read, proc_macro2::Span::call_site());
    let write_lit = LitStr::new(&schema.schema.privacy.write, proc_macro2::Span::call_site());

    let mut schema_fields = Vec::new();
    let mut edges = Vec::new();
    let connections: Vec<TokenStream> = schema
        .schema
        .connections
        .iter()
        .map(schema_connection_tokens)
        .collect();

    for (idx, field) in schema.fields.iter().enumerate() {
        let field_policies = schema.field_policies.get(idx).and_then(|p| p.as_ref());
        let (edge_ts, row_ts) = schema_field_and_edge_tokens(field, field_policies);
        if let Some(e) = edge_ts {
            edges.push(e);
        }
        schema_fields.push(row_ts);
    }

    let table_name_lit = LitStr::new(&schema.table_name, proc_macro2::Span::call_site());

    let description_code = if let Some(d) = &description_lit {
        quote! { Some(#d.to_string()) }
    } else {
        quote! { None }
    };

    let description_const_code = if let Some(d) = &description_lit {
        quote! { Some(#d) }
    } else {
        quote! { None }
    };

    let policies_code = generate_policies_code(schema.policies.as_ref());

    let trait_names_code = string_names_to_vec_code(&schema.traits);
    let side_effects_code = string_names_to_vec_code(&schema.side_effects);
    let iters_code = string_names_to_vec_code(&schema.iters);
    let composite_key_code = string_names_to_vec_code(&schema.composite_key);

    let ttl_code = if let Some(ttl) = &schema.schema.ttl {
        let seconds = ttl.seconds;
        let mode_lit = LitStr::new(&ttl.mode, proc_macro2::Span::call_site());
        quote! {
            Some(valence::SchemaTtlPolicy {
                seconds: #seconds,
                mode: #mode_lit.to_string(),
            })
        }
    } else {
        quote! { None }
    };

    let ownership_code = match &schema.schema.ownership {
        None => quote! { None },
        Some(o) => {
            let system = o.system_owned;
            let resolve = match &o.resolve {
                None => quote! { None },
                Some(s) => {
                    let lit = LitStr::new(s, proc_macro2::Span::call_site());
                    quote! { Some(#lit.to_string()) }
                }
            };
            quote! {
                Some(valence::OwnershipConfig {
                    system_owned: #system,
                    resolve: #resolve,
                })
            }
        }
    };

    let (database_evaluator_code, database_typecheck_code) =
        database_emission_tokens(schema.database.as_ref());

    SchemaMetadataPieces {
        struct_name,
        schema_struct_name,
        version_lit,
        read_lit,
        write_lit,
        table_name_lit,
        schema_fields,
        edges,
        connections,
        description_code,
        description_const_code,
        policies_code,
        trait_names_code,
        side_effects_code,
        iters_code,
        ttl_code,
        composite_key_code,
        ownership_code,
        database_evaluator_code,
        database_typecheck_code,
    }
}

fn database_emission_tokens(database: Option<&syn::Expr>) -> (TokenStream, TokenStream) {
    match database {
        None => (quote! { &valence::DEFAULT_IN_MEMORY }, quote! {}),
        Some(expr) => {
            if matches!(
                expr,
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(_),
                    ..
                })
            ) {
                // Emit a compile_error in generated code if somehow reached; callers should fail earlier.
                return (
                    quote! { &valence::DEFAULT_IN_MEMORY },
                    quote! {
                        compile_error!(
                            "`database:` cannot be a string literal (no stable address for `&dyn DatabaseEvaluator`)"
                        );
                    },
                );
            }
            (
                quote! { &#expr },
                quote! {
                    #[allow(dead_code)]
                    const _: fn() = || {
                        let _: &dyn valence::DatabaseEvaluator = &#expr;
                    };
                },
            )
        }
    }
}
