//! `Model` trait method bodies — thin orchestration over per-operation emitters.

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_create::{model_create_method_tokens, model_upsert_method_tokens};
use super::emit_ctx::CrudEmitCtx;
use super::emit_delete::model_delete_method_tokens;
use super::emit_get::model_get_method_tokens;
use super::emit_update::model_update_method_tokens;

pub(super) use super::emit_ownership::{
    emit_batch_creatable_tokens, emit_model_privacy_and_unique_impl, emit_ownership_support_tokens,
};

pub(super) fn emit_model_trait_impl(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let struct_name = &cx.struct_name;
    let schema_struct_name = &cx.schema_struct_name;
    let field_changes_name = &cx.field_changes_name;
    let table_name_lit = cx.table_name_lit;
    let version_lit = cx.version_lit;

    let get = model_get_method_tokens(cx);
    let create = model_create_method_tokens(cx);
    let update = model_update_method_tokens(field_changes_name);
    let delete = model_delete_method_tokens(cx);
    let upsert = model_upsert_method_tokens(cx);

    quote! {
        #[async_trait::async_trait]
        impl valence::Model for #struct_name {
            type Schema = #schema_struct_name;
            type FieldChanges = #field_changes_name;

            fn table_name() -> &'static str {
                #table_name_lit
            }

            fn schema_version() -> &'static str {
                #version_lit
            }

            #get
            #create
            #update
            #delete
            #upsert
        }
    }
}
