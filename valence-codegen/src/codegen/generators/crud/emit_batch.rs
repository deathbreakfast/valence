//! `batch_create` / `batch_update` token emission (rows queued on a `valence::ModelBatch`).

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_ctx::CrudEmitCtx;
use super::emit_ownership::ownership_after_row_persisted;

pub(super) fn model_batch_inherent_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let field_changes_name = &cx.field_changes_name;
    let own = ownership_after_row_persisted(cx, "created");
    quote! {
        /// Check a new row like `Model::create` does and queue it on `batch`.
        ///
        /// Create privacy, required read-hidable fields and unique constraints (against
        /// stored rows) are checked now, under the batch's actor. Nothing is written until
        /// `batch.commit()`; ownership and Create side effects run after that commit.
        pub async fn batch_create<'v>(
            data: Self,
            batch: &mut valence::ModelBatch<'v>,
            purpose: valence::DataUsePurpose,
        ) -> valence::Result<()> {
            let _ = purpose;
            let valence = batch.valence();
            data.check_create_privacy(valence).await?;
            let mut record = serde_json::to_value(&data)
                .map_err(valence::Error::from)?;
            Self::__reject_unset_read_hidable(&record)?;
            Self::__assert_unique_constraints_for_record(&record, None, valence).await?;
            let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
            valence::prepare_create_content(
                <Self as valence::Model>::table_name(),
                backend.as_ref(),
                &mut record,
            )?;
            batch.push_checked(
                valence::WriteOp::Create {
                    table: <Self as valence::Model>::table_name().to_string(),
                    content: record,
                },
                move |row| async move {
                    let created: Self = serde_json::from_value(row)
                        .map_err(valence::Error::from)?;

                    #own

                    if let Some(__rid) = created.id() {
                        valence::read_cache::invalidate(<Self as valence::Model>::table_name(), __rid.id());
                    }
                    let field_changes = #field_changes_name::compute(None, Some(&created));
                    let mutation = valence::Mutation::new(
                        valence::MutationKind::Create,
                        None,
                        Some(created.clone()),
                        field_changes,
                        valence,
                    );
                    Self::dispatch_side_effects(&mutation).await;
                    Ok(())
                },
            );
            Ok(())
        }

        /// Check a replacement row like `Model::update` does and queue it on `batch`.
        ///
        /// Update privacy is checked now against the stored row, under the batch's actor;
        /// fields the actor can't read and leaves unset keep their stored values. A missing
        /// row returns `Error::NotFound`. Nothing is written until `batch.commit()`; Update
        /// side effects run after that commit.
        pub async fn batch_update<'v>(
            id: &str,
            data: Self,
            batch: &mut valence::ModelBatch<'v>,
            purpose: valence::DataUsePurpose,
        ) -> valence::Result<()> {
            let _ = purpose;
            let valence = batch.valence();
            data.check_update_privacy(valence).await?;
            let mut record = serde_json::to_value(&data)
                .map_err(valence::Error::from)?;
            Self::__assert_unique_constraints_for_record(&record, Some(id), valence).await?;
            let __stored = valence::QueryCore::get_stored_record_json(
                <Self as valence::Model>::table_name(),
                id,
                valence,
                valence::use_!(r"When a model **queues a row update in a batch**, we **load the stored row first** so Update privacy is checked against what is actually saved. The same request path building the batch uses this load."),
            )
            .await?
            .ok_or_else(|| valence::Error::NotFound(format!(
                "{}:{}",
                <Self as valence::Model>::table_name(),
                id
            )))?;
            valence::PrivacyEvaluator::check_entity_access(
                Self::__schema_metadata()?,
                valence::PrivacyOperation::Update,
                &__stored,
                valence,
            )
            .await?;
            let (__visible, __hidden) = valence::PrivacyEvaluator::filter_entity_fields(
                Self::__schema_metadata()?,
                &__stored,
                valence.actor(),
            )?;
            let __visible = serde_json::Value::Object(__visible.into_iter().collect());
            valence::PrivacyEvaluator::check_field_update_access(
                Self::__schema_metadata()?,
                Some(&__visible),
                &record,
                valence.actor(),
            )?;
            let merge_hidden = valence::drop_unset_hidden_fields(&mut record, &__hidden)
                || Self::__lacks_read_hidable(&record);
            let before_snapshot = <Self as valence::Model>::get(
                id,
                valence,
                valence::DataUsePurpose::framework_nested(),
            )
            .await?;
            let table = <Self as valence::Model>::table_name().to_string();
            let id = id.to_string();
            // A field this viewer couldn't read is absent from `record`; merge so the
            // stored value survives.
            let op = if merge_hidden {
                valence::WriteOp::Merge { table, id, patch: record }
            } else {
                valence::WriteOp::Update { table, id, content: record }
            };
            batch.push_checked(
                op,
                move |row| async move {
                    let updated: Self = serde_json::from_value(row)
                        .map_err(valence::Error::from)?;
                    if let Some(__rid) = updated.id() {
                        valence::read_cache::invalidate(<Self as valence::Model>::table_name(), __rid.id());
                    }
                    let field_changes = #field_changes_name::compute(
                        before_snapshot.as_ref(),
                        Some(&updated),
                    );
                    let mutation = valence::Mutation::new(
                        valence::MutationKind::Update,
                        before_snapshot,
                        Some(updated),
                        field_changes,
                        valence,
                    );
                    Self::dispatch_side_effects(&mutation).await;
                    Ok(())
                },
            );
            Ok(())
        }
    }
}
