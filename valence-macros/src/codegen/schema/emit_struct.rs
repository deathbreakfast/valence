//! Inventory registration and iter hook emission for [`super`] schema expansion.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::LitStr;

use valence_schema_dsl;

pub(super) fn iter_registration_tokens(
    parsed: &valence_schema_dsl::ParsedSchema,
) -> (Vec<TokenStream2>, Vec<TokenStream2>) {
    let table_name_lit = LitStr::new(&parsed.table_name, proc_macro2::Span::call_site());
    let model_ident = super::to_pascal_case_ident(&parsed.table_name);
    let mut iter_fn_defs: Vec<TokenStream2> = Vec::new();
    let mut iter_submissions: Vec<TokenStream2> = Vec::new();

    for iter_name_str in &parsed.iters {
        let iter_ident = format_ident!("{}", iter_name_str);
        let iter_name_lit = LitStr::new(iter_name_str, proc_macro2::Span::call_site());
        let frag_iter = super::sanitize_for_rust_ident(iter_name_str);
        let frag_table = super::sanitize_for_rust_ident(&parsed.table_name);
        let should_run_fn = format_ident!("__valence_iter_should_run_{}_{}", frag_iter, frag_table);
        let execute_fn = format_ident!("__valence_iter_execute_{}_{}", frag_iter, frag_table);
        iter_fn_defs.push(quote! {
            #[cfg(not(target_family = "wasm"))]
            fn #should_run_fn(
                v: valence::Valence,
                row: serde_json::Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = valence::Result<valence::IterEvaluation>> + Send + 'static>> {
                Box::pin(async move {
                    let model: #model_ident = serde_json::from_value(row)?;
                    const _VALENCE_ITER: #iter_ident = #iter_ident;
                    _VALENCE_ITER.should_run(&model, &v).await
                })
            }
            #[cfg(not(target_family = "wasm"))]
            fn #execute_fn(
                v: valence::Valence,
                row: serde_json::Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = valence::Result<()>> + Send + 'static>> {
                Box::pin(async move {
                    let model: #model_ident = serde_json::from_value(row)?;
                    const _VALENCE_ITER: #iter_ident = #iter_ident;
                    _VALENCE_ITER.execute(&model, &v).await
                })
            }
        });
        iter_submissions.push(quote! {
            valence::inventory::submit! {
                valence::IterDescriptor {
                    iter_type_name: #iter_name_lit,
                    table_name: #table_name_lit,
                    should_run: #should_run_fn,
                    execute: #execute_fn,
                }
            }
        });
    }

    (iter_fn_defs, iter_submissions)
}

pub(super) fn trait_implementor_submissions(
    parsed: &valence_schema_dsl::ParsedSchema,
) -> Vec<TokenStream2> {
    let table_name_lit = LitStr::new(&parsed.table_name, proc_macro2::Span::call_site());
    parsed
        .traits
        .iter()
        .map(|trait_name| {
            let trait_name_lit = LitStr::new(trait_name, proc_macro2::Span::call_site());
            quote! {
                valence::inventory::submit! {
                    valence::TraitImplementor {
                        trait_name: #trait_name_lit,
                        table_name: #table_name_lit,
                    }
                }
            }
        })
        .collect()
}
