//! `IterDescriptor` inventory registration for schemas that declare `iters:`.
//!
//! Mirrors [`valence_macros::codegen::schema`] so offline/codegen paths match proc-macro output.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::LitStr;

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::{sanitize_for_rust_ident, to_pascal_case};

/// Emit type-erased iter hooks + `inventory::submit!` for each `iters:` entry.
pub fn generate_iters(schema: &SchemaContext) -> Result<TokenStream, Box<dyn std::error::Error>> {
    if schema.iters.is_empty() {
        return Ok(quote! {});
    }

    let table_name_lit = LitStr::new(&schema.table_name, proc_macro2::Span::call_site());
    let model_ident = format_ident!("{}", to_pascal_case(&schema.table_name));

    let mut iter_fn_defs = Vec::new();
    let mut iter_submissions = Vec::new();

    for iter_name_str in &schema.iters {
        let iter_ident = format_ident!("{}", iter_name_str);
        let iter_name_lit = LitStr::new(iter_name_str, proc_macro2::Span::call_site());
        let frag_iter = sanitize_for_rust_ident(iter_name_str);
        let frag_table = sanitize_for_rust_ident(&schema.table_name);
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

    Ok(quote! {
        #(#iter_fn_defs)*
        #[cfg(not(target_family = "wasm"))]
        const _: () = {
            #(#iter_submissions)*
        };
    })
}
