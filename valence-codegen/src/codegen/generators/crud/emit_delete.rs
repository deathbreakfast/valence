//! `Model::delete` / `Model::delete_now` token emission.

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_ctx::CrudEmitCtx;

pub(super) fn model_delete_method_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let field_changes_name = &cx.field_changes_name;
    let mark = ownership_mark_pending(cx);
    if cx.deletion_skip {
        return quote! {
            async fn delete(id: &str, valence: &valence::Valence) -> valence::Result<()> {
                Self::delete_now(id, valence).await
            }

            async fn delete_now(id: &str, valence: &valence::Valence) -> valence::Result<()> {
                let bare = valence::deletion::normalize_record_id_for_deletion(
                    <Self as valence::Model>::table_name(),
                    id,
                );
                let before_json = valence::QueryCore::get_record_json(
                    <Self as valence::Model>::table_name(),
                    &bare,
                    valence,
                )
                .await?;
                let Some(before_json) = before_json else {
                    valence::read_cache::invalidate(
                        <Self as valence::Model>::table_name(),
                        &bare,
                    );
                    return Ok(());
                };
                if let Some(schema) =
                    valence::SchemaRegistry::global().get_schema(<Self as valence::Model>::table_name())
                {
                    valence::PrivacyEvaluator::check_entity_access(
                        schema,
                        valence::PrivacyOperation::Delete,
                        &before_json,
                        valence,
                    )
                    .await?;
                }

                let bare_owned = bare.clone();
                valence::retry_on_database_tx_conflict("Model::delete_now", || {
                    let bare_owned = bare_owned.clone();
                    async move {
                        let backend =
                            valence.backend_for_table(<Self as valence::Model>::table_name())?;
                        backend
                            .delete_record(Self::table_name(), bare_owned.as_str())
                            .await
                    }
                })
                .await?;

                valence::read_cache::invalidate(
                    <Self as valence::Model>::table_name(),
                    bare.as_str(),
                );

                if let Ok(before) = serde_json::from_value::<Self>(before_json) {
                    let field_changes = #field_changes_name::compute(Some(&before), None);
                    let mutation = valence::Mutation::new(
                        valence::MutationKind::Delete,
                        Some(before),
                        None,
                        field_changes,
                        valence,
                    );
                    Self::dispatch_side_effects(&mutation).await;
                }

                Ok(())
            }
        };
    }

    quote! {
        async fn delete(id: &str, valence: &valence::Valence) -> valence::Result<()> {
            match valence::deletion::prepare_deletion(
                Self::table_name(),
                id,
                valence::deletion::DeletionMode::Queued,
                valence,
            )
            .await?
            {
                valence::deletion::PreparedDeletion::Missing
                | valence::deletion::PreparedDeletion::Pending { .. } => Ok(()),
                valence::deletion::PreparedDeletion::Ready { bare_id, .. } => {
                    #mark
                    let actor_json = serde_json::to_value(valence.actor())
                        .unwrap_or(serde_json::Value::Null);
                    let run_id = valence::deletion::DeletionService::create_run(
                        Self::table_name(),
                        &bare_id,
                        actor_json.clone(),
                        valence,
                    )
                    .await?;
                    valence::deletion::dispatch(valence::deletion::DeletionRequest {
                        run_id,
                        root_table: Self::table_name().to_string(),
                        root_record_id: bare_id,
                        actor_json,
                    })
                    .await?;
                    Ok(())
                }
            }
        }
    }
}

fn ownership_mark_pending(cx: &CrudEmitCtx<'_>) -> TokenStream {
    if cx.ownership_skip {
        return quote! {};
    }
    quote! {
        valence::ownership::OwnershipService::mark_pending_deletion(
            Self::table_name(),
            &bare_id,
            valence,
        )
        .await?;
    }
}
