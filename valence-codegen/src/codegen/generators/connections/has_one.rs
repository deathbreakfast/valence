//! HasOne: FK on this model — `get_*`, `get_from_*`, `*_thing`, etc.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use valence_core::SchemaConnection;

use crate::codegen::schema::SchemaContext;

use super::common::connection_target_type;

pub(super) fn push_has_one_methods_for_connection(
    schema: &SchemaContext,
    struct_name: &proc_macro2::Ident,
    conn: &SchemaConnection,
    field_names: &[&str],
    methods: &mut Vec<TokenStream>,
) {
    let from_field = &conn.from_field;
    if !field_names.contains(&from_field.as_str()) {
        return;
    }

    let field_ident = format_ident!("{}", from_field);
    let target_type = connection_target_type(conn);

    let get_method_name = format_ident!("get_{}", from_field);
    let thing_method_name = format_ident!("{}_thing", from_field);
    let get_from_method_name = format_ident!("get_from_{}", from_field);
    let get_from_id_method_name = format_ident!("get_from_{}_id", from_field);
    let where_method_name = format_ident!("where_{}", from_field);

    let field_is_nullable = schema
        .fields
        .iter()
        .find(|f| f.name == *from_field)
        .is_some_and(|f| f.nullable);

    if field_is_nullable {
        methods.push(quote! {
            /// Raw record id for the #from_field connection (FK). Returns None when the field is unset.
            pub fn #thing_method_name(&self) -> Option<&valence::RecordId> {
                self.#field_ident.as_ref()
            }
        });
    } else {
        methods.push(quote! {
            /// Raw record id for the #from_field connection (FK)
            pub fn #thing_method_name(&self) -> &valence::RecordId {
                &self.#field_ident
            }
        });
    }

    let target_table_lit = conn.to_table.as_str();
    let forward_method = if field_is_nullable {
        quote! {
            /// Navigate the `#from_field` connection. Loads full target, runs read privacy.
            /// Returns `Ok(None)` when the FK field is unset.
            pub async fn #get_method_name(&self, valence: &valence::Valence) -> valence::Result<Option<#target_type>> {
                match &self.#field_ident {
                    Some(rid) => {
                        let id = valence::connection::extract_id_from_record(rid)?;
                        #[allow(deprecated)]
                        <#target_type as valence::Model>::get(&id, valence).await
                    }
                    None => Ok(None),
                }
            }
        }
    } else if conn.required {
        quote! {
            /// Navigate the `#from_field` connection. Loads full target, runs read privacy.
            pub async fn #get_method_name(&self, valence: &valence::Valence) -> valence::Result<#target_type> {
                let id = valence::connection::extract_id_from_record(&self.#field_ident)?;
                #[allow(deprecated)]
                let row = <#target_type as valence::Model>::get(&id, valence).await?;
                row.ok_or_else(|| valence::Error::NotFound(
                        format!("{} {} not found", #target_table_lit, id),
                    ))
            }
        }
    } else {
        quote! {
            /// Navigate the `#from_field` connection. Loads full target, runs read privacy.
            pub async fn #get_method_name(&self, valence: &valence::Valence) -> valence::Result<Option<#target_type>> {
                let id = valence::connection::extract_id_from_record(&self.#field_ident)?;
                #[allow(deprecated)]
                <#target_type as valence::Model>::get(&id, valence).await
            }
        }
    };
    methods.push(forward_method);

    methods.push(quote! {
        /// Reverse: all records pointing to this target (by reference)
        pub async fn #get_from_method_name(
            target: &#target_type,
            valence: &valence::Valence,
        ) -> valence::Result<Vec<#struct_name>> {
            let id = valence::connection::id_from_model(target)?;
            Self::#get_from_id_method_name(&id, valence).await
        }
    });

    let to_table_lit = conn.to_table.as_str();
    methods.push(quote! {
        /// Reverse: all records pointing to this target (by ID)
        pub async fn #get_from_id_method_name(
            target_id: &str,
            valence: &valence::Valence,
        ) -> valence::Result<Vec<#struct_name>> {
            #[allow(deprecated)]
            #struct_name::query(valence)
                .#where_method_name(valence::RecordPredicate::Equals(
                    valence::RecordId::new(#to_table_lit, target_id),
                ))
                .await
        }
    });
}
