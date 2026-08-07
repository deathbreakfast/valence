//! `FieldChanges` diffing and `dispatch_side_effects` for models that declare `side_effects:`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::generators::rust_types::rust_type_tokens;
use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;

/// Generate `*FieldChanges` struct, its `compute` method, and side-effect dispatch
/// helper for a model.
#[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
pub fn generate_side_effects(
    schema: &SchemaContext,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let model_name = to_pascal_case(&schema.table_name);
    let field_changes_name = format_ident!("{}FieldChanges", struct_name);

    let mut field_change_defs = Vec::new();
    let mut field_change_computes = Vec::new();

    for field in &schema.fields {
        if field.primary {
            continue;
        }

        let field_name = format_ident!("{}", field.name);
        let is_required = !field.nullable;

        let field_type = rust_type_tokens(field, &model_name);

        if is_required {
            // Required field: FieldChange wraps the raw type
            field_change_defs.push(quote! {
                pub #field_name: valence::FieldChange<#field_type>
            });

            field_change_computes.push(quote! {
                #field_name: valence::FieldChange::new(
                    before.map(|b| b.#field_name().clone()),
                    after.map(|a| a.#field_name().clone()),
                )
            });
        } else {
            // Optional field: FieldChange wraps Option<T>
            field_change_defs.push(quote! {
                pub #field_name: valence::FieldChange<Option<#field_type>>
            });

            field_change_computes.push(quote! {
                #field_name: valence::FieldChange::new(
                    before.map(|b| b.#field_name().cloned()),
                    after.map(|a| a.#field_name().cloned()),
                )
            });
        }
    }

    // Generate side effect dispatch code
    let dispatch_code = generate_dispatch_code(schema);

    Ok(quote! {
        /// Per-field before/after changes for #struct_name.
        ///
        /// Each field provides a `FieldChange<T>` with `before()`, `after()`,
        /// and `has_changed()` accessors.
        #[allow(dead_code)]
        pub struct #field_changes_name {
            #(#field_change_defs),*
        }

        impl #field_changes_name {
            /// Compute field changes from before/after model snapshots.
            pub fn compute(before: Option<&#struct_name>, after: Option<&#struct_name>) -> Self {
                Self {
                    #(#field_change_computes),*
                }
            }
        }

        #dispatch_code
    })
}

/// Generate the side effect dispatch helper as a private method on the model.
fn generate_dispatch_code(schema: &SchemaContext) -> TokenStream {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let field_changes_name = format_ident!("{}FieldChanges", struct_name);

    // Build the list of side effect type instantiations
    let effect_instantiations: Vec<TokenStream> = schema
        .side_effects
        .iter()
        .map(|name| {
            let effect_path = name.parse::<TokenStream>().unwrap_or_else(|_| {
                let effect_ident = format_ident!("{}", name);
                quote! { #effect_ident }
            });
            quote! {
                Box::new(#effect_path) as Box<dyn valence::SideEffect<Self>>
            }
        })
        .collect();

    if effect_instantiations.is_empty() {
        // No side effects registered -- generate a no-op dispatch method
        return quote! {
            impl #struct_name {
                /// Dispatch registered side effects (none registered).
                #[allow(unused_variables)]
                async fn dispatch_side_effects(
                    mutation: &valence::Mutation<'_, Self>,
                ) {
                    // No side effects registered for this model.
                }
            }
        };
    }

    let table_name_lit = syn::LitStr::new(&schema.table_name, proc_macro2::Span::call_site());
    let frag_table = crate::codegen::utils::sanitize_for_rust_ident(&schema.table_name);
    let dispatch_fn = format_ident!("__valence_delete_side_effect_dispatch_{}", frag_table);

    quote! {
        impl #struct_name {
            /// Dispatch registered side effects after a successful mutation.
            ///
            /// Errors are logged but do not fail the mutation.
            async fn dispatch_side_effects(
                mutation: &valence::Mutation<'_, Self>,
            ) {
                let kind_str = match mutation.kind() {
                    valence::MutationKind::Create => "create",
                    valence::MutationKind::Update => "update",
                    valence::MutationKind::Delete => "delete",
                };
                valence::instrumentation::record_side_effect_dispatch(
                    <Self as valence::Model>::table_name(),
                    kind_str,
                );
                let side_effects: Vec<Box<dyn valence::SideEffect<Self>>> = vec![
                    #(#effect_instantiations),*
                ];
                for se in &side_effects {
                    if let Err(e) = se.on_mutation(mutation).await {
                        valence::instrumentation::record_side_effect_error(
                            <Self as valence::Model>::table_name(),
                            &e.to_string(),
                        );
                    }
                }
            }
        }

        #[cfg(not(target_family = "wasm"))]
        fn #dispatch_fn(
            v: valence::Valence,
            row: serde_json::Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
            Box::pin(async move {
                let before: #struct_name = match serde_json::from_value(row) {
                    Ok(m) => m,
                    Err(e) => {
                        valence::instrumentation::record_side_effect_error(
                            #table_name_lit,
                            &e.to_string(),
                        );
                        return;
                    }
                };
                let field_changes = #field_changes_name::compute(Some(&before), None);
                let mutation = valence::Mutation::new(
                    valence::MutationKind::Delete,
                    Some(before),
                    None,
                    field_changes,
                    &v,
                );
                #struct_name::dispatch_side_effects(&mutation).await;
            })
        }

        #[cfg(not(target_family = "wasm"))]
        const _: () = {
            valence::inventory::submit! {
                valence::deletion::DeleteSideEffectDescriptor {
                    table_name: #table_name_lit,
                    dispatch: #dispatch_fn,
                }
            }
        };
    }
}
