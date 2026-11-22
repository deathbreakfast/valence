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
            Self::__reject_unset_read_hidable(&record)?;
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
            // Decide create vs update from the row actually in storage. `Model::get`
            // hides rows the caller can't read, and treating such a row as absent
            // would let create policies (checked on the caller's own payload)
            // authorize overwriting someone else's row.
            let __stored = valence::QueryCore::get_stored_record_json(
                <Self as valence::Model>::table_name(),
                id,
                valence,
                valence::use_!(r"When a model **upserts a row**, we **load the stored row first** so Valence knows whether this is a create or an update and checks the right policy against what is actually saved. The same request path performing the upsert uses this load."),
            )
            .await?;
            let mut record = serde_json::to_value(&data)
                .map_err(valence::Error::from)?;
            let merge_existing = if let Some(__stored) = &__stored {
                // A row the caller can't read is refused outright, never created over.
                valence::PrivacyEvaluator::check_entity_access(
                    Self::__schema_metadata()?,
                    valence::PrivacyOperation::Read,
                    __stored,
                    valence,
                )
                .await?;
                valence::PrivacyEvaluator::check_entity_access(
                    Self::__schema_metadata()?,
                    valence::PrivacyOperation::Update,
                    __stored,
                    valence,
                )
                .await?;
                data.check_update_privacy(valence).await?;
                // Compare against the stored row with this viewer's unreadable fields
                // left out, so a field it couldn't read never looks cleared.
                let (__visible, __hidden) = valence::PrivacyEvaluator::filter_entity_fields(
                    Self::__schema_metadata()?,
                    __stored,
                    valence.actor(),
                )?;
                let __visible = serde_json::Value::Object(__visible.into_iter().collect());
                valence::PrivacyEvaluator::check_field_update_access(
                    Self::__schema_metadata()?,
                    Some(&__visible),
                    &record,
                    valence.actor(),
                )?;
                valence::drop_unset_hidden_fields(&mut record, &__hidden)
                    || Self::__lacks_read_hidable(&record)
            } else {
                data.check_create_privacy(valence).await?;
                Self::__reject_unset_read_hidable(&record)?;
                false
            };
            let before_snapshot = if __stored.is_some() {
                Self::get(id, valence, purpose).await?
            } else {
                None
            };
            let exists = __stored.is_some();
            Self::__assert_unique_constraints_for_record(&record, Some(id), valence).await?;

            let id = id.to_string();
            let upserted: Self = valence::retry_on_database_tx_conflict("Model::upsert", || {
                let id = id.clone();
                let record = record.clone();
                let creating = !exists;
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
                    let row = if merge_existing {
                        backend
                            .merge_record(Self::table_name(), id.as_str(), record)
                            .await?
                    } else {
                        backend
                            .upsert_record(Self::table_name(), id.as_str(), record)
                            .await?
                    };
                    serde_json::from_value(row)
                        .map_err(valence::Error::from)
                }
            })
            .await?;

            if !exists {
                #own_create
            }

            {
                let kind = if exists {
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
