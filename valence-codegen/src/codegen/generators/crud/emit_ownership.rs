//! Ownership hooks, privacy checks, and unique-constraint inherent helpers.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::LitStr;

use super::emit_ctx::CrudEmitCtx;
use super::emit_update::model_update_with_before_inherent_tokens;

pub(super) fn ownership_resolution_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let system_lit = syn::LitBool::new(cx.ownership_system_owned, proc_macro2::Span::call_site());
    match &cx.ownership_resolver {
        None => quote! {
            if let Some(o) = valence.owner_override() {
                o.clone()
            } else if #system_lit {
                valence::OwnerRef::system()
            } else {
                valence::OwnerRef::from_actor(valence.actor())
            }
        },
        Some(path) => quote! {
            if let Some(o) = valence.owner_override() {
                o.clone()
            } else if #system_lit {
                valence::OwnerRef::system()
            } else {
                #path::default()
                    .resolve_owner(&__payload, valence.actor(), valence)
                    .await?
            }
        },
    }
}

pub(super) fn ownership_after_row_persisted(cx: &CrudEmitCtx<'_>, row_ident: &str) -> TokenStream {
    if cx.ownership_skip {
        return quote! {};
    }
    let row = format_ident!("{}", row_ident);
    quote! {
        #row.__ensure_ownership_after_write(valence).await?;
    }
}

/// Inherent ownership helper (shared with [`BatchCreatable::ensure_ownership_after_batch_create`]).
pub(super) fn emit_ownership_support_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    if cx.ownership_skip {
        return quote! {};
    }
    let struct_name = &cx.struct_name;
    let table_name_lit = cx.table_name_lit;
    let resolve = ownership_resolution_tokens(cx);
    quote! {
        #[allow(dead_code)]
        impl #struct_name {
            pub(crate) async fn __ensure_ownership_after_write(
                &self,
                valence: &valence::Valence,
            ) -> valence::Result<()> {
                let __rec_id = self.id().ok_or_else(|| {
                    valence::Error::Validation(format!(
                        "{}: record has no id after write",
                        #table_name_lit,
                    ))
                })?;
                let __key = __rec_id.id().to_string();
                let __bare = valence::ownership::normalize_record_id_for_ownership(&__key);
                let __payload = serde_json::to_value(self)
                    .map_err(valence::Error::from)?;
                let __owner: valence::OwnerRef = #resolve;
                valence::ownership::OwnershipService::ensure_active_ownership(
                    #table_name_lit,
                    &__bare,
                    __owner,
                    valence,
                )
                .await
            }
        }
    }
}

/// Single [`BatchCreatable`] impl (table routing + optional batch ownership hook).
pub(super) fn emit_batch_creatable_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let struct_name = &cx.struct_name;
    let table_name_lit = cx.table_name_lit;
    let ownership_hook = if cx.ownership_skip {
        quote! {}
    } else {
        quote! {
            async fn ensure_ownership_after_batch_create(
                created_row: serde_json::Value,
                valence: &valence::Valence,
            ) -> valence::Result<()> {
                let row: Self = serde_json::from_value(created_row)
                    .map_err(valence::Error::from)?;
                row.__ensure_ownership_after_write(valence).await
            }
        }
    };
    quote! {
        #[async_trait::async_trait]
        impl valence::BatchCreatable for #struct_name {
            fn table_name() -> &'static str {
                #table_name_lit
            }

            fn set_id(&mut self, id: valence::RecordId) {
                self.id = Some(id);
            }

            #ownership_hook
        }
    }
}

fn schema_metadata_fn_tokens(table_name_lit: &str) -> TokenStream {
    quote! {
        fn __schema_metadata() -> valence::Result<&'static valence::SchemaMetadataStruct> {
            valence::SchemaRegistry::global()
                .get_schema(#table_name_lit)
                .ok_or_else(|| {
                    valence::Error::Internal(
                        concat!("SchemaRegistry missing entry for ", #table_name_lit).into(),
                    )
                })
        }
    }
}

fn unique_constraint_helpers_tokens(unique_field_names: &[LitStr]) -> TokenStream {
    quote! {
        async fn __assert_unique_constraints_for_record(
            record: &serde_json::Value,
            excluding_id: Option<&str>,
            valence: &valence::Valence,
        ) -> valence::Result<()> {
            for field_name in Self::__unique_field_names() {
                let Some(value) = record.get(field_name) else {
                    continue;
                };
                if value.is_null() {
                    continue;
                }
                Self::__assert_unique_field_value(field_name, value, excluding_id, valence)
                    .await?;
            }
            Ok(())
        }

        fn __unique_field_names() -> &'static [&'static str] {
            &[#(#unique_field_names),*]
        }

        async fn __assert_unique_field_value(
            field_name: &str,
            field_value: &serde_json::Value,
            excluding_id: Option<&str>,
            valence: &valence::Valence,
        ) -> valence::Result<()> {
            valence
                .ensure_unique_field_index(<Self as valence::Model>::table_name(), field_name)
                .await?;
            let compiled = valence::__internal::CompiledQuery {
                // Prefer plain SQL `SELECT id` (typed columns). Surreal compilers still
                // accept this shape; document-era VALUE rewrite remains as a thin shim.
                query_string: format!(
                    "SELECT id FROM {} WHERE {} = $value LIMIT 2",
                    <Self as valence::Model>::table_name(),
                    field_name,
                ),
                params: vec![("value".to_string(), field_value.clone())],
            };
            let matches: Vec<serde_json::Value> = valence::retry_on_database_tx_conflict(
                "Model::__assert_unique_field_value",
                || {
                    let compiled = compiled.clone();
                    async move {
                        let backend = valence
                            .backend_for_table(<Self as valence::Model>::table_name())?;
                        backend.execute_compiled_query(&compiled).await
                    }
                },
            )
            .await?;
            for row in matches {
                let matched_id = valence::extract_id_from_select_value(&row).map_err(|e| {
                    valence::Error::Internal(format!("Failed to parse unique check ID: {}", e))
                })?;
                if excluding_id.is_some_and(|id| id == matched_id) {
                    continue;
                }
                return Err(valence::Error::Validation(format!(
                    "Unique constraint violation on {}.{}",
                    <Self as valence::Model>::table_name(),
                    field_name
                )));
            }
            Ok(())
        }
    }
}

fn entity_privacy_check_tokens() -> TokenStream {
    quote! {
        async fn check_read_privacy(&self, valence: &valence::Valence) -> valence::Result<()> {
            let record = serde_json::json!(self);
            valence::PrivacyEvaluator::check_entity_access(
                Self::__schema_metadata()?, valence::PrivacyOperation::Read, &record, valence,
            )
            .await
        }

        async fn check_create_privacy(&self, valence: &valence::Valence) -> valence::Result<()> {
            let record = serde_json::json!(self);
            valence::PrivacyEvaluator::check_entity_access(
                Self::__schema_metadata()?, valence::PrivacyOperation::Create, &record, valence,
            )
            .await
        }

        async fn check_update_privacy(&self, valence: &valence::Valence) -> valence::Result<()> {
            let record = serde_json::json!(self);
            valence::PrivacyEvaluator::check_entity_access(
                Self::__schema_metadata()?, valence::PrivacyOperation::Update, &record, valence,
            )
            .await
        }

        async fn check_delete_privacy(&self, valence: &valence::Valence) -> valence::Result<()> {
            let record = serde_json::json!(self);
            valence::PrivacyEvaluator::check_entity_access(
                Self::__schema_metadata()?, valence::PrivacyOperation::Delete, &record, valence,
            )
            .await
        }
    }
}

pub(super) fn emit_model_privacy_and_unique_impl(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let struct_name = &cx.struct_name;
    let field_changes_name = &cx.field_changes_name;
    let table_name_lit = cx.table_name_lit;
    let unique_field_names = &cx.unique_field_names;

    let meta = schema_metadata_fn_tokens(table_name_lit);
    let unique = unique_constraint_helpers_tokens(unique_field_names);
    let privacy = entity_privacy_check_tokens();
    let update_with_before = model_update_with_before_inherent_tokens(field_changes_name);

    quote! {
        impl #struct_name {
            #meta
            #unique
            #privacy
            #update_with_before
        }
    }
}
