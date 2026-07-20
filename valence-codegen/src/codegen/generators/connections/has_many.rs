//! HasMany: FK on the target table — reverse `get_*` via `where_{reverse_field}`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use valence_core::SchemaConnection;

use crate::codegen::schema::SchemaContext;

use super::common::connection_target_type;

pub(super) fn push_has_many_method_for_connection(
    schema: &SchemaContext,
    conn: &SchemaConnection,
    methods: &mut Vec<TokenStream>,
) {
    let Some(reverse_field) = &conn.reverse_field else {
        return;
    };
    let conn_name = &conn.from_field;
    let target_type = connection_target_type(conn);

    let get_method_name = format_ident!("get_{}", conn_name);
    let where_method_name = format_ident!("where_{}", reverse_field);
    let self_table_lit = schema.table_name.as_str();

    methods.push(quote! {
        /// Navigate the `#conn_name` connection (HasMany). Loads related records, runs read privacy.
        pub async fn #get_method_name(&self, valence: &valence::Valence) -> valence::Result<Vec<#target_type>> {
            let id = valence::connection::id_from_model(self)?;
            #target_type::query(valence)
                .#where_method_name(valence::RecordPredicate::Equals(
                    valence::RecordId::new(#self_table_lit, &id),
                ))
                .await
        }
    });
}
