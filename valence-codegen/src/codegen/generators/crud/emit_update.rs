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

        let before_snapshot = match before {
            Some(existing) => {
                existing.check_update_privacy(valence).await?;
                Some(existing)
            }
            None => {
                let fetched = <Self as valence::Model>::get(id, valence).await?;
                if let Some(ref existing) = fetched {
                    existing.check_update_privacy(valence).await?;
                }
                fetched
            }
        };

        let id = id.to_string();
        let updated: Self = valence::retry_on_database_tx_conflict("Model::update", || {
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
        .await?;

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
        /// Update an existing record, optionally skipping the pre-update fetch when the caller
        /// already holds the authoritative row (e.g. `get_mutable().commit()`).
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
        async fn update(id: &str, data: Self, valence: &valence::Valence) -> valence::Result<Self> {
            Self::update_with_before(id, data, None, valence).await
        }
    }
}
