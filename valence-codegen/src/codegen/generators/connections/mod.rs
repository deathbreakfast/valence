//! Connection navigation methods for Valence models.
//!
//! Generates `get_{name}`, `get_from_{name}`, `get_from_{name}_id` for HasOne
//! connections, ManyToMany helpers, HasMany reverse queries, and `IdHolder`.

mod common;
mod has_many;
mod has_one;
mod many_to_many;
mod trait_target_has_many;
mod trait_target_has_one;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;

use self::has_many::push_has_many_method_for_connection;
use self::has_one::push_has_one_methods_for_connection;
use self::many_to_many::{
    push_many_to_many_concrete_methods, push_many_to_many_trait_target_methods,
};
use self::trait_target_has_many::push_has_many_trait_target_method;
use self::trait_target_has_one::push_has_one_trait_target_methods;

/// Generate connection navigation methods for models with HasOne connections
/// (FK on this model). Also generates IdHolder impl.
#[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
pub fn generate_connections(
    schema: &SchemaContext,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));

    let mut methods = Vec::new();
    let mut id_holder_impl = TokenStream::new();

    let has_one_connections: Vec<_> = schema
        .schema
        .connections
        .iter()
        .filter(|c| c.cardinality == "HasOne" && c.target_trait.is_none())
        .collect();

    let has_one_trait_connections: Vec<_> = schema
        .schema
        .connections
        .iter()
        .filter(|c| c.cardinality == "HasOne" && c.target_trait.is_some())
        .collect();

    let has_many_connections: Vec<_> = schema
        .schema
        .connections
        .iter()
        .filter(|c| c.cardinality == "HasMany" && c.target_trait.is_none())
        .collect();

    let has_many_trait_connections: Vec<_> = schema
        .schema
        .connections
        .iter()
        .filter(|c| c.cardinality == "HasMany" && c.target_trait.is_some())
        .collect();

    let field_names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();

    for conn in has_one_connections {
        push_has_one_methods_for_connection(schema, &struct_name, conn, &field_names, &mut methods);
    }

    for conn in has_one_trait_connections {
        push_has_one_trait_target_methods(schema, &struct_name, conn, &field_names, &mut methods);
    }

    let many_to_many_connections: Vec<_> = schema
        .schema
        .connections
        .iter()
        .filter(|c| {
            c.cardinality == "ManyToMany" && c.edge_table.is_some() && c.target_trait.is_none()
        })
        .collect();
    let trait_many_to_many_connections: Vec<_> = schema
        .schema
        .connections
        .iter()
        .filter(|c| {
            c.cardinality == "ManyToMany" && c.edge_table.is_some() && c.target_trait.is_some()
        })
        .collect();

    for conn in many_to_many_connections {
        push_many_to_many_concrete_methods(conn, &mut methods);
    }

    for conn in trait_many_to_many_connections {
        push_many_to_many_trait_target_methods(conn, &mut methods);
    }

    for conn in has_many_connections {
        push_has_many_method_for_connection(schema, conn, &mut methods);
    }

    for conn in has_many_trait_connections {
        push_has_many_trait_target_method(schema, conn, &mut methods);
    }

    if schema.fields.iter().any(|f| f.primary) {
        id_holder_impl = quote! {
            impl valence::connection::IdHolder for #struct_name {
                fn record_id(&self) -> Option<&valence::RecordId> {
                    self.id()
                }
            }
        };
    }

    if methods.is_empty() && id_holder_impl.is_empty() {
        return Ok(TokenStream::new());
    }

    let methods_block = if methods.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            #[allow(dead_code)]
            impl #struct_name {
                #(#methods)*
            }
        }
    };

    Ok(quote! {
        #methods_block
        #id_holder_impl
    })
}
