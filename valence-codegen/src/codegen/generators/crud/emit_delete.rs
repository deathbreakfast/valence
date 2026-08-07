//! `Model::delete` token emission.

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_ctx::CrudEmitCtx;

pub(super) fn model_delete_method_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let field_changes_name = &cx.field_changes_name;
    let mark = ownership_mark_pending(cx);
    if cx.deletion_skip {
        return quote! {
            async fn delete(id: &str, valence: &valence::Valence) -> valence::Result<()> {
                let before_snapshot = Self::get(id, valence).await?;
                if let Some(ref existing) = before_snapshot {
                    existing.check_delete_privacy(valence).await?;
                }

                let id = id.to_string();
                valence::retry_on_database_tx_conflict("Model::delete", || {
                    let id = id.clone();
                    async move {
                        let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
                        backend
                            .delete_record(Self::table_name(), id.as_str())
                            .await
                    }
                })
                .await?;

                valence::read_cache::invalidate(<Self as valence::Model>::table_name(), id.as_str());

                if let Some(ref before) = before_snapshot {
                    let field_changes = #field_changes_name::compute(
                        Some(before),
                        None,
                    );
                    let mutation = valence::Mutation::new(
                        valence::MutationKind::Delete,
                        before_snapshot,
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
            let before_snapshot = match Self::get(id, valence).await {
                Ok(s) => s,
                Err(valence::Error::PendingDeletion(_)) => return Ok(()),
                Err(e) => return Err(e),
            };
            if let Some(ref existing) = before_snapshot {
                existing.check_delete_privacy(valence).await?;
            } else {
                return Ok(());
            }

            let __bare = valence::ownership::normalize_record_id_for_ownership(id);
            let dag = valence::deletion::dag::DeletionDag::compute(Self::table_name(), &__bare, valence)
                .await?;
            if !dag.restrict_violations.is_empty() {
                return Err(valence::Error::Validation(format!(
                    "delete restricted by schema connections: {:?}",
                    dag.restrict_violations
                )));
            }
            valence::deletion::check_dag_delete_privacy(&dag, valence).await?;

            #mark

            let actor_json = serde_json::to_value(valence.actor())
                .unwrap_or(serde_json::Value::Null);
            let run_id = valence::deletion::DeletionService::create_run(
                Self::table_name(),
                &__bare,
                actor_json.clone(),
                valence,
            )
            .await?;
            valence::deletion::dispatch(valence::deletion::DeletionRequest {
                run_id,
                root_table: Self::table_name().to_string(),
                root_record_id: __bare,
                actor_json,
            })
            .await?;

            Ok(())
        }
    }
}

fn ownership_mark_pending(cx: &CrudEmitCtx<'_>) -> TokenStream {
    if cx.ownership_skip {
        return quote! {};
    }
    quote! {
        if before_snapshot.is_some() {
            let __bare = valence::ownership::normalize_record_id_for_ownership(id);
            valence::ownership::OwnershipService::mark_pending_deletion(
                Self::table_name(),
                &__bare,
                valence,
            )
            .await?;
        }
    }
}
