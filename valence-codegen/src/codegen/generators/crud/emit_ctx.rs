//! Inputs shared by CRUD token emitters (`Model`, privacy, mutable builder).

use proc_macro2::TokenStream;
use syn::LitStr;

use super::mutable_parts::MutablePartStreams;

pub(super) struct CrudEmitCtx<'a> {
    pub struct_name: proc_macro2::Ident,
    pub schema_struct_name: proc_macro2::Ident,
    pub mutable_name: proc_macro2::Ident,
    pub table_name_lit: &'a str,
    pub version_lit: &'a str,
    pub field_changes_name: proc_macro2::Ident,
    pub unique_field_names: Vec<LitStr>,
    /// Required fields with a field-level read policy (stored as `Option`).
    pub read_hidable_required_names: Vec<LitStr>,
    pub parts: &'a MutablePartStreams,
    pub composite_key_methods: TokenStream,
    /// Skip ownership hooks for platform ownership tables.
    pub ownership_skip: bool,
    /// Use synchronous hard-delete (no queued deletion pipeline).
    pub deletion_skip: bool,
    pub ownership_system_owned: bool,
    /// Parsed `ownership.resolve` type path (Rust), if any.
    pub ownership_resolver: Option<syn::Path>,
}
