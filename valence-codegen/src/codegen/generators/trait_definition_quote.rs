//! Builds `valence_trait_schema!` output: enums, `*Fields`, `*Model`, `*Query`, `*QueryAll`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::LitStr;

use crate::codegen::parser::ParsedTraitDef;
use crate::codegen::utils::to_pascal_case;

use super::enums::generate_enum_definition;
use super::trait_helpers::{predicate_type_for, rust_type_for_field, where_core_call_for};
use super::trait_query_connections::collect_trait_query_all_hop_methods;

/// Collected token fragments for one trait definition file.
struct TraitDefinitionPieces {
    fields_trait_name: proc_macro2::Ident,
    query_trait_name: proc_macro2::Ident,
    model_struct_name: proc_macro2::Ident,
    query_all_struct_name: proc_macro2::Ident,
    trait_name_lit: LitStr,
    enum_definitions: Vec<TokenStream>,
    fields_trait_getters: Vec<TokenStream>,
    model_field_defs: Vec<TokenStream>,
    model_getters: Vec<TokenStream>,
    query_trait_methods: Vec<TokenStream>,
    query_trait_order_by_methods: Vec<TokenStream>,
    query_all_where_methods_pub: Vec<TokenStream>,
    query_all_where_methods_trait: Vec<TokenStream>,
    query_all_order_by_methods_pub: Vec<TokenStream>,
    query_all_order_by_methods_trait: Vec<TokenStream>,
    query_all_hop_methods: Vec<TokenStream>,
    select_field_lits: Vec<LitStr>,
}

fn collect_enum_definitions(
    trait_name: &str,
    fields: &[valence_core::SchemaField],
) -> Vec<TokenStream> {
    let mut out = Vec::new();
    for field in fields {
        if field.field_type.starts_with("enum:") && field.enum_type.is_none() {
            let enum_name = format!("{}{}", trait_name, to_pascal_case(&field.name));
            let variants = &field.enum_variants;
            if !variants.is_empty() {
                out.push(generate_enum_definition(&enum_name, variants));
            }
        }
    }
    out
}

fn non_primary_fields(
    fields: &[valence_core::SchemaField],
) -> impl Iterator<Item = &valence_core::SchemaField> {
    fields.iter().filter(|f| !f.primary)
}

fn collect_trait_definition_pieces(trait_def: &ParsedTraitDef) -> TraitDefinitionPieces {
    let trait_name = trait_def.name.as_str();
    let fields_trait_name = format_ident!("{}Fields", trait_name);
    let query_trait_name = format_ident!("{}Query", trait_name);
    let model_struct_name = format_ident!("{}Model", trait_name);
    let query_all_struct_name = format_ident!("{}QueryAll", trait_name);
    let trait_name_lit = LitStr::new(trait_name, proc_macro2::Span::call_site());

    let enum_definitions = collect_enum_definitions(trait_name, &trait_def.fields);

    let fields_trait_getters: Vec<TokenStream> = non_primary_fields(&trait_def.fields)
        .map(|f| {
            let getter = format_ident!("{}", f.name);
            let rtype = rust_type_for_field(f, trait_name);
            if f.nullable {
                quote! { fn #getter(&self) -> Option<&#rtype>; }
            } else {
                quote! { fn #getter(&self) -> &#rtype; }
            }
        })
        .collect();

    let model_field_defs: Vec<TokenStream> = non_primary_fields(&trait_def.fields)
        .map(|f| {
            let name = format_ident!("{}", f.name);
            let rtype = rust_type_for_field(f, trait_name);
            if f.nullable {
                quote! { #name: Option<#rtype> }
            } else {
                quote! { #name: #rtype }
            }
        })
        .collect();

    let model_getters: Vec<TokenStream> = non_primary_fields(&trait_def.fields)
        .map(|f| {
            let getter = format_ident!("{}", f.name);
            let rtype = rust_type_for_field(f, trait_name);
            if f.nullable {
                quote! {
                    fn #getter(&self) -> Option<&#rtype> { self.#getter.as_ref() }
                }
            } else {
                quote! {
                    fn #getter(&self) -> &#rtype { &self.#getter }
                }
            }
        })
        .collect();

    let query_trait_methods: Vec<TokenStream> = non_primary_fields(&trait_def.fields)
        .map(|f| {
            let method = format_ident!("where_{}", f.name);
            let pred_type = predicate_type_for(&f.field_type);
            quote! { fn #method(self, predicate: #pred_type) -> Self; }
        })
        .collect();

    let query_all_where_methods_pub: Vec<TokenStream> = non_primary_fields(&trait_def.fields)
        .map(|f| {
            let method = format_ident!("where_{}", f.name);
            let pred_type = predicate_type_for(&f.field_type);
            let core_method = where_core_call_for(&f.field_type);
            let field_name_lit = LitStr::new(&f.name, proc_macro2::Span::call_site());
            quote! {
                pub fn #method(mut self, predicate: #pred_type) -> Self {
                    self.inner = self.inner.#core_method(#field_name_lit.to_string(), predicate);
                    self
                }
            }
        })
        .collect();

    let query_all_where_methods_trait: Vec<TokenStream> = non_primary_fields(&trait_def.fields)
        .map(|f| {
            let method = format_ident!("where_{}", f.name);
            let pred_type = predicate_type_for(&f.field_type);
            let core_method = where_core_call_for(&f.field_type);
            let field_name_lit = LitStr::new(&f.name, proc_macro2::Span::call_site());
            quote! {
                fn #method(mut self, predicate: #pred_type) -> Self {
                    self.inner = self.inner.#core_method(#field_name_lit.to_string(), predicate);
                    self
                }
            }
        })
        .collect();

    let query_trait_order_by_methods: Vec<TokenStream> = trait_def
        .fields
        .iter()
        .map(|f| {
            let method = format_ident!("order_by_{}", f.name);
            quote! { fn #method(self, direction: valence::SortDirection) -> Self; }
        })
        .collect();

    let query_all_order_by_methods_pub: Vec<TokenStream> = trait_def
        .fields
        .iter()
        .map(|f| {
            let method = format_ident!("order_by_{}", f.name);
            let field_name_lit = LitStr::new(&f.name, proc_macro2::Span::call_site());
            quote! {
                pub fn #method(mut self, direction: valence::SortDirection) -> Self {
                    self.inner = self.inner.order_by(#field_name_lit.to_string(), direction);
                    self
                }
            }
        })
        .collect();

    let query_all_order_by_methods_trait: Vec<TokenStream> = trait_def
        .fields
        .iter()
        .map(|f| {
            let method = format_ident!("order_by_{}", f.name);
            let field_name_lit = LitStr::new(&f.name, proc_macro2::Span::call_site());
            quote! {
                fn #method(mut self, direction: valence::SortDirection) -> Self {
                    self.inner = self.inner.order_by(#field_name_lit.to_string(), direction);
                    self
                }
            }
        })
        .collect();

    let select_field_lits: Vec<LitStr> = std::iter::once("id".to_string())
        .chain(non_primary_fields(&trait_def.fields).map(|f| f.name.clone()))
        .map(|s| LitStr::new(&s, proc_macro2::Span::call_site()))
        .collect();

    let query_all_hop_methods =
        collect_trait_query_all_hop_methods(trait_name, &trait_def.connections);

    TraitDefinitionPieces {
        fields_trait_name,
        query_trait_name,
        model_struct_name,
        query_all_struct_name,
        trait_name_lit,
        enum_definitions,
        fields_trait_getters,
        model_field_defs,
        model_getters,
        query_trait_methods,
        query_trait_order_by_methods,
        query_all_where_methods_pub,
        query_all_where_methods_trait,
        query_all_order_by_methods_pub,
        query_all_order_by_methods_trait,
        query_all_hop_methods,
        select_field_lits,
    }
}

fn quote_trait_definition_bundle(p: &TraitDefinitionPieces) -> TokenStream {
    let fields_trait_name = &p.fields_trait_name;
    let query_trait_name = &p.query_trait_name;
    let model_struct_name = &p.model_struct_name;
    let query_all_struct_name = &p.query_all_struct_name;
    let trait_name_lit = &p.trait_name_lit;
    let enum_definitions = &p.enum_definitions;
    let fields_trait_getters = &p.fields_trait_getters;
    let model_field_defs = &p.model_field_defs;
    let model_getters = &p.model_getters;
    let query_trait_methods = &p.query_trait_methods;
    let query_trait_order_by_methods = &p.query_trait_order_by_methods;
    let query_all_where_methods_pub = &p.query_all_where_methods_pub;
    let query_all_where_methods_trait = &p.query_all_where_methods_trait;
    let query_all_order_by_methods_pub = &p.query_all_order_by_methods_pub;
    let query_all_order_by_methods_trait = &p.query_all_order_by_methods_trait;
    let query_all_hop_methods = &p.query_all_hop_methods;
    let select_field_lits = &p.select_field_lits;

    quote! {
        #(#enum_definitions)*

        pub trait #fields_trait_name {
            #(#fields_trait_getters)*
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct #model_struct_name {
            pub id: Option<valence::RecordId>,
            #(#model_field_defs),*
        }

        impl #fields_trait_name for #model_struct_name {
            #(#model_getters)*
        }

        pub trait #query_trait_name<'a>: Sized {
            #(#query_trait_methods)*
            #(#query_trait_order_by_methods)*
            fn into_parts(self) -> (valence::QueryCore, &'a valence::Valence);
            fn from_parts(inner: valence::QueryCore, valence: &'a valence::Valence) -> Self;
        }

        #[derive(Clone)]
        #[allow(dead_code)]
        pub struct #query_all_struct_name<'a> {
            pub inner: valence::QueryCore,
            pub valence: &'a valence::Valence,
        }

        #[allow(dead_code)]
        impl<'a> #query_all_struct_name<'a> {
            pub fn query(valence: &'a valence::Valence) -> Self {
                let tables = valence::TraitRegistry::global()
                    .tables_for_trait(#trait_name_lit);
                let table_csv = tables.join(", ");
                let mut core = valence::QueryCore::new(table_csv);
                core.projection = Some(vec![#(#select_field_lits.to_string()),*]);
                Self { inner: core, valence }
            }

            #(#query_all_where_methods_pub)*

            #(#query_all_order_by_methods_pub)*

            #(#query_all_hop_methods)*

            pub fn limit(mut self, limit: u32) -> Self {
                self.inner = self.inner.limit(limit);
                self
            }

            pub fn offset(mut self, offset: u32) -> Self {
                self.inner = self.inner.offset(offset);
                self
            }

            pub async fn first(self) -> valence::Result<Option<#model_struct_name>> {
                let mut results = self.limit(1).await?;
                Ok(results.pop())
            }

            pub fn into_parts(self) -> (valence::QueryCore, &'a valence::Valence) {
                (self.inner, self.valence)
            }

            pub fn from_parts(inner: valence::QueryCore, valence: &'a valence::Valence) -> Self {
                Self { inner, valence }
            }
        }

        impl<'a> #query_trait_name<'a> for #query_all_struct_name<'a> {
            #(#query_all_where_methods_trait)*

            #(#query_all_order_by_methods_trait)*

            fn into_parts(self) -> (valence::QueryCore, &'a valence::Valence) {
                (self.inner, self.valence)
            }

            fn from_parts(inner: valence::QueryCore, valence: &'a valence::Valence) -> Self {
                Self { inner, valence }
            }
        }

        impl<'a> std::future::IntoFuture for #query_all_struct_name<'a> {
            type Output = valence::Result<Vec<#model_struct_name>>;
            type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

            fn into_future(self) -> Self::IntoFuture {
                Box::pin(async move {
                    self.inner.execute(self.valence).await
                })
            }
        }
    }
}

#[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
pub fn generate_trait_definition(
    trait_def: &ParsedTraitDef,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let pieces = collect_trait_definition_pieces(trait_def);
    Ok(quote_trait_definition_bundle(&pieces))
}
