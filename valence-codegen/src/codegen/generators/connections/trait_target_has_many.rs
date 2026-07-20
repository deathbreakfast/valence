//! HasMany trait-target reverse: `get_*` via trait union query builders.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use valence_core::SchemaConnection;

use crate::codegen::schema::SchemaContext;

pub(super) fn push_has_many_trait_target_method(
    schema: &SchemaContext,
    conn: &SchemaConnection,
    methods: &mut Vec<TokenStream>,
) {
    let Some(target_trait) = conn.target_trait.as_deref() else {
        return;
    };
    let Some(reverse_field) = conn.reverse_field.as_deref() else {
        return;
    };

    let conn_name = &conn.from_field;
    let get_method_name = format_ident!("get_{}", conn_name);
    let where_method_name = format_ident!("where_{}", reverse_field);
    let target_query_all = format_ident!("{}QueryAll", target_trait);
    let target_model = format_ident!("{}Model", target_trait);
    let self_table_lit = schema.table_name.as_str();

    methods.push(quote! {
        /// Navigate the `#conn_name` connection (HasMany trait target).
        /// Returns rows from all trait implementor tables for this source.
        pub async fn #get_method_name(
            &self,
            valence: &valence::Valence,
        ) -> valence::Result<Vec<#target_model>> {
            let id = valence::connection::id_from_model(self)?;
            let parent_rid = valence::RecordId::new(#self_table_lit, &id);
            #target_query_all::query(valence)
                .#where_method_name(valence::RecordPredicate::Equals(parent_rid))
                .await
        }
    });
}
