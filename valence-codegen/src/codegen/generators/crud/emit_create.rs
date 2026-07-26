//! `Model::create`, `upsert`, and `merge` token emission.

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_ctx::CrudEmitCtx;
use super::emit_ownership::ownership_after_row_persisted;

pub(super) fn model_create_method_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let field_changes_name = &cx.field_changes_name;
    let own = ownership_after_row_persisted(cx, "created");
    quote! {
        async fn create(data: Self, valence: &valence::Valence) -> valence::Result<Self> {
            data.check_create_privacy(valence).await?;
            let record = serde_json::to_value(&data)
                .map_err(valence::Error::from)?;
            Self::__assert_unique_constraints_for_record(&record, None, valence).await?;

            let created: Self = valence::retry_on_database_tx_conflict("Model::create", || {
                let record = record.clone();
                async move {
                    let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
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
        async fn upsert(id: &str, data: Self, valence: &valence::Valence) -> valence::Result<Self> {
            let before_snapshot = Self::get(id, valence).await?;
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
                async move {
                    let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
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

pub(super) fn model_merge_method_tokens(field_changes_name: &proc_macro2::Ident) -> TokenStream {
    quote! {
        async fn merge(
            id: &str,
            patch: serde_json::Value,
            valence: &valence::Valence,
        ) -> valence::Result<Self> {
            let before_snapshot = Self::get(id, valence).await?;
            let Some(ref existing) = before_snapshot else {
                return Err(valence::Error::NotFound(format!(
                    "{}:{}",
                    Self::table_name(),
                    id
                )));
            };
            existing.check_update_privacy(valence).await?;

            let patch_for_db = match &patch {
                serde_json::Value::Object(obj) if obj.is_empty() => {
                    return Ok(existing.clone());
                }
                serde_json::Value::Object(_) => patch.clone(),
                _ => {
                    return Err(valence::Error::Validation(
                        "Model::merge expects a JSON object patch".into(),
                    ));
                }
            };

            let mut merged_json = serde_json::to_value(existing)
                .map_err(valence::Error::from)?;
            if let serde_json::Value::Object(ref patch_obj) = patch_for_db {
                if let serde_json::Value::Object(ref mut base) = merged_json {
                    for (k, v) in patch_obj {
                        base.insert(k.clone(), v.clone());
                    }
                } else {
                    return Err(valence::Error::Internal(
                        "Model::merge: failed to merge into record JSON".into(),
                    ));
                }
            }

            Self::__assert_unique_constraints_for_record(&merged_json, Some(id), valence).await?;

            let proposed: Self = serde_json::from_value(merged_json)
                .map_err(valence::Error::from)?;
            proposed.check_update_privacy(valence).await?;

            let id = id.to_string();
            let merged: Self = valence::retry_on_database_tx_conflict("Model::merge", || {
                let id = id.clone();
                let patch_for_db = patch_for_db.clone();
                async move {
                    let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
                    let row = backend
                        .merge_record(Self::table_name(), id.as_str(), patch_for_db)
                        .await?;
                    serde_json::from_value(row)
                        .map_err(valence::Error::from)
                }
            })
            .await?;

            {
                let field_changes = #field_changes_name::compute(
                    before_snapshot.as_ref(),
                    Some(&merged),
                );
                let mutation = valence::Mutation::new(
                    valence::MutationKind::Update,
                    before_snapshot,
                    Some(merged.clone()),
                    field_changes,
                    valence,
                );
                Self::dispatch_side_effects(&mutation).await;
            }

            if let Some(__rid) = merged.id() {
                valence::read_cache::invalidate(<Self as valence::Model>::table_name(), __rid.id());
            }

            Ok(merged)
        }
    }
}
