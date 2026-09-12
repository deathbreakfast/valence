//! Builds `valence::Schema` values, graph edges, connections, and `inventory` registration
//! for [`crate::valence_schema`].

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::LitStr;

use crate::codegen::policies;
use valence_schema_dsl;

mod emit_connections;
mod emit_fields;
mod emit_struct;

use emit_connections::connections_tokens;
use emit_fields::{edges_tokens, fields_tokens};
use emit_struct::{iter_registration_tokens, trait_implementor_submissions};

/// Expand `valence_schema! { ... }` into static schema metadata + trait implementor hooks.
pub fn expand(input: TokenStream) -> TokenStream {
    let dsl = match syn::parse::<valence_schema_dsl::SchemaSpec>(input) {
        Ok(dsl) => dsl,
        Err(e) => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("Failed to parse schema DSL: {e}"),
            )
            .to_compile_error()
            .into();
        }
    };

    let parsed = match dsl.to_schema() {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };

    expand_parsed_schema(&parsed).into()
}

fn expand_parsed_schema(parsed: &valence_schema_dsl::ParsedSchema) -> TokenStream2 {
    let table_name_lit = LitStr::new(&parsed.table_name, proc_macro2::Span::call_site());
    let version_lit = LitStr::new(&parsed.version, proc_macro2::Span::call_site());

    let description_code = if let Some(desc) = &parsed.description {
        let desc_lit = LitStr::new(desc, proc_macro2::Span::call_site());
        quote! { Some(#desc_lit.to_string()) }
    } else {
        quote! { None }
    };
    let repository_lit = LitStr::new(&parsed.repository, proc_macro2::Span::call_site());
    let retention_lit = LitStr::new(&parsed.retention, proc_macro2::Span::call_site());
    let owner_lit = LitStr::new(&parsed.owner, proc_macro2::Span::call_site());

    let privacy_read_lit = LitStr::new("public", proc_macro2::Span::call_site());
    let privacy_write_lit = LitStr::new("service", proc_macro2::Span::call_site());

    let (database_static, database_value, database_evaluator_typecheck) = match parsed
        .database
        .as_ref()
    {
        None => (quote! {}, quote! { valence::DEFAULT_IN_MEMORY }, quote! {}),
        Some(expr) => {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(_),
                ..
            }) = expr
            {
                return syn::Error::new_spanned(
                        expr,
                        "`database:` cannot be a string literal (no stable address for `&dyn DatabaseEvaluator`). \
Use a named `const` in your crate, e.g. \
`pub const MY_DB: valence::DatabaseFromEngine = valence::Database::from_engine(\"primary\", \"my_engine\");` \
then `database: crate::MY_DB`.",
                    )
                    .to_compile_error();
            }
            (
                quote! {},
                quote! { #expr },
                database_evaluator_typecheck_tokens(Some(expr)),
            )
        }
    };

    let policies_code = policies::policies_tokens(parsed.policies.as_ref());
    let ttl_code = ttl_tokens(parsed.ttl.as_ref());
    let fields_code = fields_tokens(&parsed.fields);
    let edges_code = edges_tokens(&parsed.fields);
    let connections_code =
        connections_tokens(&parsed.fields, &parsed.connections, &parsed.table_name);
    let side_effects_code = string_vec_tokens(&parsed.side_effects);
    let iters_code = string_vec_tokens(&parsed.iters);
    let composite_key_code = string_vec_tokens(&parsed.composite_key);
    let traits_code = string_vec_tokens(&parsed.traits);

    let ownership_code = match &parsed.ownership {
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

    let schema_code = quote! {{
        let __valence_db_eval: &'static dyn valence::DatabaseEvaluator = &(#database_value);
        valence::Schema {
            name: #table_name_lit.to_string(),
            version: #version_lit.to_string(),
            database_evaluator: __valence_db_eval,
            databases: vec![__valence_db_eval.name().to_string()],
            privacy: valence::SchemaPrivacy {
                read: #privacy_read_lit.to_string(),
                write: #privacy_write_lit.to_string(),
            },
            policies: #policies_code,
            fields: vec![#(#fields_code),*],
            edges: vec![#(#edges_code),*],
            connections: vec![#(#connections_code),*],
            side_effects: #side_effects_code,
            iters: #iters_code,
            composite_key: #composite_key_code,
            traits: #traits_code,
            ttl: #ttl_code,
            ownership: #ownership_code,
            meta: valence::SchemaMeta {
                retention: #retention_lit.to_string(),
                row_count: 0,
                owner: #owner_lit.to_string(),
                description: #description_code,
                repository: #repository_lit.to_string(),
            },
        }
    }};

    let trait_implementor_submissions = trait_implementor_submissions(parsed);
    let (iter_fn_defs, iter_submissions) = iter_registration_tokens(parsed);

    quote! {
        #[cfg(not(target_family = "wasm"))]
        #database_static

        #(#iter_fn_defs)*

        #[cfg(not(target_family = "wasm"))]
        const _: () = {
            #database_evaluator_typecheck

            fn __schema_metadata() -> &'static valence::SchemaMetadataStruct {
                use std::sync::OnceLock;
                static SCHEMA: OnceLock<valence::Schema> = OnceLock::new();
                static METADATA: OnceLock<valence::SchemaMetadataStruct> = OnceLock::new();
                METADATA.get_or_init(|| {
                    let schema = SCHEMA.get_or_init(|| #schema_code);
                    valence::SchemaMetadataStruct {
                        table_name: schema.name.as_str(),
                        version: schema.version.as_str(),
                        description: schema.meta.description.as_deref(),
                        privacy_read: schema.privacy.read.as_str(),
                        privacy_write: schema.privacy.write.as_str(),
                        databases: schema.databases.as_slice(),
                        schema,
                    }
                })
            }

            valence::inventory::submit! {
                valence::SchemaMetadataInit(__schema_metadata)
            }

            #(#trait_implementor_submissions)*

            #(#iter_submissions)*
        };
    }
}

pub(super) fn sanitize_for_rust_ident(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

pub(super) fn to_pascal_case_ident(table_name: &str) -> syn::Ident {
    let pascal: String = table_name
        .split('_')
        .map(|part| {
            let mut ch = part.chars();
            match ch.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
            }
        })
        .collect();
    format_ident!("{}", pascal, span = proc_macro2::Span::call_site())
}

fn database_evaluator_typecheck_tokens(database: Option<&syn::Expr>) -> TokenStream2 {
    let Some(expr) = database else {
        return quote! {};
    };
    quote! {
        #[allow(dead_code)]
        const _: fn() = || {
            let _: &dyn valence::DatabaseEvaluator = &#expr;
        };
    }
}

pub(super) fn string_vec_tokens(values: &[String]) -> TokenStream2 {
    if values.is_empty() {
        return quote! { Vec::new() };
    }

    let lits: Vec<LitStr> = values
        .iter()
        .map(|value| LitStr::new(value, proc_macro2::Span::call_site()))
        .collect();

    quote! { vec![#(#lits.to_string()),*] }
}

fn ttl_tokens(ttl: Option<&valence_schema_dsl::ParsedTtlPolicy>) -> TokenStream2 {
    let Some(ttl) = ttl else {
        return quote! { None };
    };
    let seconds = ttl.seconds;
    let mode_lit = LitStr::new(&ttl.mode, proc_macro2::Span::call_site());
    quote! {
        Some(valence::SchemaTtlPolicy {
            seconds: #seconds,
            mode: #mode_lit.to_string(),
        })
    }
}
