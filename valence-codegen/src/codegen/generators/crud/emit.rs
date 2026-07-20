//! Assembles the full `Model` impl, privacy helpers, and mutable builder struct.

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_ctx::CrudEmitCtx;
use super::emit_model_ops::{
    emit_batch_creatable_tokens, emit_model_privacy_and_unique_impl, emit_model_trait_impl,
    emit_ownership_support_tokens,
};

pub(super) fn emit_crud_tokens(ctx: &CrudEmitCtx<'_>) -> TokenStream {
    let model = emit_model_trait_impl(ctx);
    let helpers = emit_model_privacy_and_unique_impl(ctx);
    let ownership = emit_ownership_support_tokens(ctx);
    let batch_creatable = emit_batch_creatable_tokens(ctx);
    let mutable = emit_mutable_builder_and_composite(ctx);
    quote! {
        #model
        #helpers
        #ownership
        #batch_creatable
        #mutable
    }
}

/// Mutable builder struct/impl, `get_mutable`, and composite-key inherent impl.
fn emit_mutable_builder_and_composite(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let struct_name = &cx.struct_name;
    let mutable_name = &cx.mutable_name;
    let table_name_lit = cx.table_name_lit;
    let parts = cx.parts;
    let composite_key_methods = &cx.composite_key_methods;

    let mutable_fields = &parts.mutable_fields;
    let mutable_field_names = &parts.mutable_field_names;
    let setter_methods = &parts.setter_methods;
    let clear_methods = &parts.clear_methods;
    let has_change_methods = &parts.has_change_methods;
    let commit_updates = &parts.commit_updates;

    quote! {
        /// Mutable builder for #struct_name.
        ///
        /// Holds a `&Valence` reference and the entity ID so that
        /// `commit().await` can persist changes without a separate
        /// `Model::update()` call.
        #[allow(dead_code)]
        pub struct #mutable_name<'a> {
            model: #struct_name,
            valence: &'a valence::Valence,
            id: Option<String>,
            #(#mutable_fields),*
        }

        #[allow(dead_code)]
        impl<'a> #mutable_name<'a> {
            /// Create a new mutable builder from a model and valence instance.
            pub fn new(model: #struct_name, valence: &'a valence::Valence) -> Self {
                let id = model.id().map(|r| r.id().to_string());
                Self {
                    model,
                    valence,
                    id,
                    #(#mutable_field_names: None),*
                }
            }

            /// Load entity from DB directly as mutable.
            pub async fn get(id: &str, valence: &'a valence::Valence) -> valence::Result<Self> {
                let model = <#struct_name as valence::Model>::get(id, valence).await?
                    .ok_or_else(|| valence::Error::Validation(
                        format!("Entity not found: {}:{}", #table_name_lit, id)
                    ))?;
                Ok(Self::new(model, valence))
            }

            #(#setter_methods)*

            #(#clear_methods)*

            #(#has_change_methods)*

            /// Apply changes locally without persisting to the database.
            pub fn build(mut self) -> #struct_name {
                #(#commit_updates)*
                self.model
            }

            /// Apply changes and persist to the database.
            pub async fn commit(self) -> valence::Result<#struct_name> {
                let id = self.id.clone()
                    .ok_or_else(|| valence::Error::Validation(
                        "Cannot commit: model has no ID".to_string()
                    ))?;
                let valence = self.valence;
                let before = self.model.clone();
                let data = self.build();
                #struct_name::update_with_before(&id, data, Some(before), valence).await
            }
        }

        impl #struct_name {
            /// Get a mutable builder for this model.
            pub fn get_mutable<'a>(&self, valence: &'a valence::Valence) -> #mutable_name<'a> {
                #mutable_name::new(self.clone(), valence)
            }
        }

        #composite_key_methods
    }
}
