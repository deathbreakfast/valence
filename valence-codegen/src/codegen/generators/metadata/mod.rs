//! `schema_metadata()` / registry payload: columns, policies, TTL, connections, composite keys.

mod collect;
mod connections;
mod fields;
mod inventory;
mod policies;
mod quote;
mod string_helpers;

use proc_macro2::TokenStream;

use crate::codegen::schema::SchemaContext;

use collect::collect_schema_metadata_pieces;
use inventory::{
    quote_connections_overlay, quote_schema_metadata_inventory, quote_trait_implementor_inventory,
};
use quote::quote_schema_metadata_method;

#[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
pub fn generate_schema_metadata_method(
    schema: &SchemaContext,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let pieces = collect_schema_metadata_pieces(schema);
    let mut tokens = quote_schema_metadata_method(&pieces);
    tokens.extend(quote_schema_metadata_inventory(&pieces));
    if !schema.traits.is_empty() {
        tokens.extend(quote_connections_overlay(&pieces));
        tokens.extend(quote_trait_implementor_inventory(
            schema,
            &pieces.table_name_lit,
        ));
    }
    Ok(tokens)
}
