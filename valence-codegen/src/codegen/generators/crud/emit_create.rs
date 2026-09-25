//! `Model::create`, `upsert`, and `merge` token emission.

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_ctx::CrudEmitCtx;
use super::emit_ownership::ownership_after_row_persisted;

pub(super) fn model_create_method_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let field_changes_name = &cx.field_changes_name;
    let own = ownership_after_row_persisted(cx, "created");
    quote! {
        async fn create(
            data: Self,
            valence: &valence::Valence,
            purpose: valence::DataUsePurpose,
        ) -> valence::Result<Self> {
            let _ = purpose;
            data.check_create_privacy(valence).await?;
            let record = serde_json::to_value(&data)
                .map_err(valence::Error::from)?;
            Self::__assert_unique_constraints_for_record(&record, None, valence).await?;

            let created: Self = valence::retry_on_database_tx_conflict("Model::create", || {
                let record = record.clone();
                async move {
                    let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
                    let mut record = record;
                    valence::prepare_create_content(
                        <Self as valence::Model>::table_name(),
                        backend.as_ref(),
                        &mut record,
                    )?;
                    let row = backend
                        .create_record(Self::table_name(), record)
                        .await?;
                    serde_json::from_value(row)
                        .map_err(valence::Error::from)
                }
            })
            .await?;

            #own

            if let Some(__rid) = created.id() {
                valence::read_cache::invalidate(<Self as valence::Model>::table_name(), __rid.id());
            }

            {
                let field_changes = #field_changes_name::compute(None, Some(&created));
                let mutation = valence::Mutation::new(
                    valence::MutationKind::Create,
                    None,
                    Some(created.clone()),
                    field_changes,
                    valence,
                );
                Self::dispatch_side_effects(&mutation).await;
            }

            Ok(created)
        }
    }
}

pub(super) fn model_upsert_method_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let field_changes_name = &cx.field_changes_name;
    let own_create = ownership_after_row_persisted(cx, "upserted");
    quote! {
        async fn upsert(
            id: &str,
            data: Self,
            valence: &valence::Valence,
            purpose: valence::DataUsePurpose,
        ) -> valence::Result<Self> {
            let before_snapshot = Self::get(id, valence, purpose).await?;
            if let Some(ref existing) = before_snapshot {
                existing.check_update_privacy(valence).await?;
                data.check_update_privacy(valence).await?;
            } else {
                data.check_create_privacy(valence).await?;
            }
            let record = serde_json::to_value(&data)
                .map_err(valence::Error::from)?;
            Self::__assert_unique_constraints_for_record(&record, Some(id), valence).await?;

            let id = id.to_string();
            let upserted: Self = valence::retry_on_database_tx_conflict("Model::upsert", || {
                let id = id.clone();
                let record = record.clone();
                let creating = before_snapshot.is_none();
                async move {
                    let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
                    let mut record = record;
                    if creating {
                        valence::prepare_create_content(
                            <Self as valence::Model>::table_name(),
                            backend.as_ref(),
                            &mut record,
                        )?;
                    }
                    let row = backend
                        .upsert_record(Self::table_name(), id.as_str(), record)
                        .await?;
                    serde_json::from_value(row)
                        .map_err(valence::Error::from)
                }
            })
            .await?;

            if before_snapshot.is_none() {
                #own_create
            }

            {
                let kind = if before_snapshot.is_some() {
                    valence::MutationKind::Update
                } else {
                    valence::MutationKind::Create
                };
                let field_changes = #field_changes_name::compute(
                    before_snapshot.as_ref(),
                    Some(&upserted),
                );
                let mutation = valence::Mutation::new(
                    kind,
                    before_snapshot,
                    Some(upserted.clone()),
                    field_changes,
                    valence,
                );
                Self::dispatch_side_effects(&mutation).await;
            }

            if let Some(__rid) = upserted.id() {
                valence::read_cache::invalidate(<Self as valence::Model>::table_name(), __rid.id());
            }

            Ok(upserted)
        }
    }
}
