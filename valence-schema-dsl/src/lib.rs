//! Shared syn-based parser for `valence_schema!` and `valence_trait_schema!`.
//!
//! Used by:
//! - [`valence-macros`](../valence_macros) — proc-macro expansion
//! - [`valence-codegen`](../valence_codegen) — build-time model generation from schema files
//!
//! This crate depends only on `syn` / `quote` / `proc-macro2` (no `valence-core` or public crate).
//!
//! # Examples
//!
//! Parse macro tokens:
//!
//! ```
//! use valence_schema_dsl::{parse_schema, SchemaSpec};
//!
//! let tokens: proc_macro2::TokenStream = syn::parse_str(
//!     r#"
//!     Counter {
//!         table: "counter",
//!         version: "0.1.0",
//!         fields: [
//!             id: { r#type: FieldType::String, primary_key: true, required: true },
//!         ],
//!     }
//!     "#,
//! )
//! .expect("tokens");
//! let parsed = parse_schema(tokens).expect("parse");
//! assert_eq!(parsed.table_name, "counter");
//! let _ = std::any::type_name::<SchemaSpec>();
//! ```
//!
//! Parse a host schema file:
//!
//! ```
//! use valence_schema_dsl::parse_schema_file;
//!
//! let src = r#"
//!     valence_schema! {
//!         Widget {
//!             table: "widget",
//!             version: "0.1.0",
//!             fields: [
//!                 id: { r#type: FieldType::String, primary_key: true, required: true },
//!             ],
//!         }
//!     }
//! "#;
//! let parsed = parse_schema_file(src).expect("parse");
//! assert_eq!(parsed.table_name, "widget");
//! ```

mod extract;
mod file;
pub mod parse;
mod trait_schema;

#[cfg(test)]
mod parse_tests;

pub use extract::{
    extract_default_string, extract_field_type, extract_field_type_string,
    extract_validator_string, ExtractedFieldType,
};
pub use file::{parse_schema_file, parse_trait_file, FileParseError};
pub use parse::*;
pub use trait_schema::{ParsedTraitSchema, TraitSchemaItem, TraitSchemaSpec};

use proc_macro2::TokenStream;

/// Parse `valence_schema!` body tokens into a [`ParsedSchema`].
///
/// # Errors
///
/// Returns a [`syn::Error`] when the DSL is invalid.
pub fn parse_schema(tokens: TokenStream) -> syn::Result<ParsedSchema> {
    let spec: SchemaSpec = syn::parse2(tokens)?;
    spec.to_schema()
}

/// Parse `valence_trait_schema!` body tokens into a [`ParsedTraitSchema`].
///
/// # Errors
///
/// Returns a [`syn::Error`] when the DSL is invalid.
pub fn parse_trait_schema(tokens: TokenStream) -> syn::Result<ParsedTraitSchema> {
    let spec: TraitSchemaSpec = syn::parse2(tokens)?;
    spec.to_parsed()
}
