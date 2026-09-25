//! `Model::update` token emission.

use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn model_update_with_before_body_tokens(
    field_changes_name: &proc_macro2::Ident,
) -> TokenStream {
    quote! {
        data.check_update_privacy(valence).await?;
        let record = serde_json::to_value(&data)
            .map_err(valence::Error::from)?;
        Self::__assert_unique_constraints_for_record(&record, Some(id), valence).await?;

        // When the caller supplies a real prior snapshot (`before`), only the fields
        // that actually differ from it are written — a sparse patch that can't clobber
        // a field it doesn't name, no matter what else changed concurrently. Diffing
        // against a freshly-refetched row instead (the `None` arm below) would not be
        // safe for this purpose, since a field the caller never touched could still
        // appear "changed" relative to a fresh read if someone else wrote it in the
        // meantime — so the `None` arm keeps doing a full-row replace, unchanged.
        let (before_snapshot, sparse_patch): (Option<Self>, Option<serde_json::Map<String, serde_json::Value>>) =
            match before {
                Some(existing) => {
                    existing.check_update_privacy(valence).await?;
                    let existing_value = serde_json::to_value(&existing)
                        .map_err(valence::Error::from)?;
                    let mut patch = serde_json::Map::new();
                    if let (
                        serde_json::Value::Object(existing_obj),
                        serde_json::Value::Object(record_obj),
                    ) = (&existing_value, &record)
                    {
                        for (k, v) in record_obj {
                            if k != "id" && existing_obj.get(k) != Some(v) {
                                patch.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    (Some(existing), Some(patch))
                }
                None => {
                    let fetched = <Self as valence::Model>::get(
                        id,
                        valence,
                        valence::DataUsePurpose::framework_nested(),
                    ).await?;
                    if let Some(ref existing) = fetched {
                        existing.check_update_privacy(valence).await?;
                    }
                    (fetched, None)
                }
            };

        let id = id.to_string();

        let updated: Self = match sparse_patch {
            Some(patch) if patch.is_empty() => {
                before_snapshot.clone().ok_or_else(|| valence::Error::NotFound(format!(
                    "{}:{}",
                    <Self as valence::Model>::table_name(),
                    id
                )))?
            }
            Some(patch) => {
                let patch_value = serde_json::Value::Object(patch);
                valence::retry_on_database_tx_conflict("Model::update_with_before", || {
                    let id = id.clone();
                    let patch_value = patch_value.clone();
                    async move {
                        let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
                        let row = backend
                            .merge_record(<Self as valence::Model>::table_name(), id.as_str(), patch_value)
                            .await?;
                        serde_json::from_value(row)
                            .map_err(valence::Error::from)
                    }
                })
                .await?
            }
            None => {
                valence::retry_on_database_tx_conflict("Model::update", || {
                    let id = id.clone();
                    let record = record.clone();
                    async move {
                        let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
                        let row = backend
                            .update_record(<Self as valence::Model>::table_name(), id.as_str(), record)
                            .await?;
                        serde_json::from_value(row)
                            .map_err(valence::Error::from)
                    }
                })
                .await?
            }
        };

        if let Some(__rid) = updated.id() {
            valence::read_cache::invalidate(<Self as valence::Model>::table_name(), __rid.id());
        }

        {
            let field_changes = #field_changes_name::compute(
                before_snapshot.as_ref(),
                Some(&updated),
            );
            let mutation = valence::Mutation::new(
                valence::MutationKind::Update,
                before_snapshot,
                Some(updated.clone()),
                field_changes,
                valence,
            );
            Self::dispatch_side_effects(&mutation).await;
        }

        Ok(updated)
    }
}

pub(super) fn model_update_with_before_inherent_tokens(
    field_changes_name: &proc_macro2::Ident,
) -> TokenStream {
    let body = model_update_with_before_body_tokens(field_changes_name);
    quote! {
        /// Update an existing record. When `before` is the caller's real original
        /// snapshot (e.g. from `get_mutable().commit()`), only the fields that
        /// changed relative to it are written — a sparse write that can't clobber a
        /// field it doesn't name. When `before` is `None`, the full row is replaced,
        /// same as `Model::update`.
        pub async fn update_with_before(
            id: &str,
            data: Self,
            before: Option<Self>,
            valence: &valence::Valence,
        ) -> valence::Result<Self> {
            #body
        }

    }
}

pub(super) fn model_update_method_tokens(_field_changes_name: &proc_macro2::Ident) -> TokenStream {
    quote! {
        async fn update(
            id: &str,
            data: Self,
            valence: &valence::Valence,
            purpose: valence::DataUsePurpose,
        ) -> valence::Result<Self> {
            let _ = purpose;
            Self::update_with_before(id, data, None, valence).await
        }
    }
}
