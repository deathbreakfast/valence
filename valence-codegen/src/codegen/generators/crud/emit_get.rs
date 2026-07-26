//! `Model::get` token emission.

use proc_macro2::TokenStream;
use quote::quote;

use super::emit_ctx::CrudEmitCtx;

pub(super) fn model_get_method_tokens(cx: &CrudEmitCtx<'_>) -> TokenStream {
    let fetch_body = if cx.deletion_skip {
        quote! {
            let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
            let row = valence::read_cache::get_record_via_cache(
                    &backend,
                    Self::table_name(),
                    id.as_str(),
                )
                .await?;
            let result = match row {
                None => None,
                Some(v) => Some(
                    serde_json::from_value(v).map_err(valence::Error::from)?,
                ),
            };
            Ok((result, valence::ownership::OwnershipGateStatus::Absent))
        }
    } else {
        quote! {
            let (row, bundle_status) = if valence::ownership::ownership_unified_fetch_enabled()
                && valence::ownership::ownership_colocate_enabled()
                && valence::schema::SchemaRegistry::global().has_schema(Self::table_name())
            {
                let bundle = valence::ownership::OwnershipService::fetch_record_with_ownership_gate(
                    Self::table_name(),
                    id.as_str(),
                    valence,
                )
                .await?;
                (bundle.row, bundle.ownership_status)
            } else {
                let backend = valence.backend_for_table(<Self as valence::Model>::table_name())?;
                let row = valence::read_cache::get_record_via_cache(
                        &backend,
                        Self::table_name(),
                        id.as_str(),
                    )
                    .await?;
                (row, valence::ownership::OwnershipGateStatus::NotFetched)
            };
            let result = match row {
                None => None,
                Some(v) => Some(
                    serde_json::from_value(v).map_err(valence::Error::from)?,
                ),
            };
            Ok((result, bundle_status))
        }
    };

    let privacy_and_gate = if cx.deletion_skip {
        quote! {
            if let Some(record) = &result {
                record.check_read_privacy(valence).await?;
            }
        }
    } else {
        quote! {
            if let Some(record) = &result {
                let __bare = valence::ownership::normalize_record_id_for_ownership(id.as_str());
                if __bundle_status == valence::ownership::OwnershipGateStatus::NotFetched {
                    valence::ownership::OwnershipService::check_privacy_with_pending_gate(
                        record.check_read_privacy(valence),
                        Self::table_name(),
                        &__bare,
                        valence,
                    )
                    .await?;
                } else {
                    valence::ownership::OwnershipService::check_privacy_with_bundled_gate(
                        record.check_read_privacy(valence),
                        Self::table_name(),
                        &__bare,
                        __bundle_status,
                    )
                    .await?;
                }
            }
        }
    };
    quote! {
        async fn get(id: &str, valence: &valence::Valence) -> valence::Result<Option<Self>> {
            let id = id.to_string();
            let (result, __bundle_status): (
                Option<Self>,
                valence::ownership::OwnershipGateStatus,
            ) = valence::retry_on_database_tx_conflict("Model::get", || {
                let id = id.clone();
                async move {
                    #fetch_body
                }
            })
            .await?;

            #privacy_and_gate

            let result = match result {
                Some(record) => {
                    let raw = serde_json::to_value(&record).map_err(valence::Error::from)?;
                    let (filtered, _) = valence::PrivacyEvaluator::filter_entity_fields(
                        Self::__schema_metadata()?,
                        &raw,
                        valence.actor(),
                    )?;
                    let filtered_json =
                        serde_json::Value::Object(filtered.into_iter().collect());
                    Some(serde_json::from_value(filtered_json).map_err(valence::Error::from)?)
                }
                None => None,
            };

            Ok(result)
        }
    }
}
