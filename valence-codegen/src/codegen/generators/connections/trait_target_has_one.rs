//! HasOne trait-target: `*_thing` on the FK field (typed load via `resolve_*` in product crates or hops).

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use valence_core::SchemaConnection;

use crate::codegen::schema::SchemaContext;

pub(super) fn push_has_one_trait_target_methods(
    schema: &SchemaContext,
    _struct_name: &proc_macro2::Ident,
    conn: &SchemaConnection,
    field_names: &[&str],
    methods: &mut Vec<TokenStream>,
) {
    let from_field = &conn.from_field;
    if !field_names.contains(&from_field.as_str()) {
        return;
    }
    if conn.target_trait.is_none() {
        return;
    }

    let field_ident = format_ident!("{}", from_field);
    let thing_method_name = format_ident!("{}_thing", from_field);

    let field_is_nullable = schema
        .fields
        .iter()
        .find(|f| f.name == *from_field)
        .is_some_and(valence_core::SchemaField::stored_optional);

    if field_is_nullable {
        methods.push(quote! {
            /// Raw record id for the `#from_field` trait-target connection.
            pub fn #thing_method_name(&self) -> Option<&valence::RecordId> {
                self.#field_ident.as_ref()
            }
        });
    } else {
        methods.push(quote! {
            /// Raw record id for the `#from_field` trait-target connection.
            pub fn #thing_method_name(&self) -> &valence::RecordId {
                &self.#field_ident
            }
        });
    }
}
