//! Per-schema trait impls on concrete models and query builders (`NamedFields`, `NamedQuery`, refine).

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::LitStr;

use crate::codegen::parser::ParsedTraitDef;
use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;
use valence_core::SchemaField;

use super::trait_helpers::{predicate_type_for, rust_type_for_field, where_core_call_for};

fn non_primary(fields: &[SchemaField]) -> impl Iterator<Item = &SchemaField> {
    fields.iter().filter(|f| !f.primary)
}

fn schema_trait_field_getter_impls(
    trait_def: &ParsedTraitDef,
    trait_name: &str,
) -> Vec<TokenStream> {
    non_primary(&trait_def.fields)
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
        .collect()
}

fn schema_trait_query_where_delegates(trait_def: &ParsedTraitDef) -> Vec<TokenStream> {
    non_primary(&trait_def.fields)
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
        .collect()
}

fn schema_trait_query_order_by_delegates(trait_def: &ParsedTraitDef) -> Vec<TokenStream> {
    trait_def
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
        .collect()
}

fn query_builder_parts_bridge_tokens(query_name: &proc_macro2::Ident) -> TokenStream {
    quote! {
        impl<'a> #query_name<'a> {
            pub fn into_parts(self) -> (valence::QueryCore, &'a valence::Valence) {
                (self.inner, self.valence)
            }

            pub fn from_parts(inner: valence::QueryCore, valence: &'a valence::Valence) -> Self {
                Self { inner, valence }
            }
        }
    }
}

fn schema_trait_named_fields_impl(
    struct_name: &proc_macro2::Ident,
    fields_trait: &proc_macro2::Ident,
    field_getters: &[TokenStream],
) -> TokenStream {
    quote! {
        impl #fields_trait for #struct_name {
            #(#field_getters)*
        }
    }
}

fn schema_trait_named_query_impl(
    query_name: &proc_macro2::Ident,
    query_trait: &proc_macro2::Ident,
    query_where_methods: &[TokenStream],
    query_order_by_methods: &[TokenStream],
) -> TokenStream {
    quote! {
        impl<'a> #query_trait<'a> for #query_name<'a> {
            #(#query_where_methods)*

            #(#query_order_by_methods)*

            fn into_parts(self) -> (valence::QueryCore, &'a valence::Valence) {
                (self.inner, self.valence)
            }

            fn from_parts(inner: valence::QueryCore, valence: &'a valence::Valence) -> Self {
                Self { inner, valence }
            }
        }
    }
}

fn schema_trait_refine_bridge(
    query_name: &proc_macro2::Ident,
    query_trait: &proc_macro2::Ident,
    query_all_struct: &proc_macro2::Ident,
    refine_trait_name: &proc_macro2::Ident,
    refine_method: &proc_macro2::Ident,
    table_lit: &LitStr,
) -> TokenStream {
    quote! {
        pub trait #refine_trait_name<'a> {
            fn #refine_method(self) -> #query_name<'a>;
        }

        impl<'a> #refine_trait_name<'a> for #query_all_struct<'a> {
            fn #refine_method(self) -> #query_name<'a> {
                let (mut inner, valence) = <Self as #query_trait<'a>>::into_parts(self);
                inner.table = #table_lit.to_string();
                inner.projection = None;
                #query_name::from_parts(inner, valence)
            }
        }
    }
}

/// `NamedFields`, `NamedQuery`, and refinement trait for one trait name on a concrete model.
fn schema_trait_impl_bundle(
    struct_name: &proc_macro2::Ident,
    query_name: &proc_macro2::Ident,
    table_snake: &str,
    trait_name: &str,
    trait_def: &ParsedTraitDef,
) -> TokenStream {
    let fields_trait = format_ident!("{}Fields", trait_name);
    let query_trait = format_ident!("{}Query", trait_name);
    let query_all_struct = format_ident!("{}QueryAll", trait_name);

    let field_getters = schema_trait_field_getter_impls(trait_def, trait_name);
    let query_where_methods = schema_trait_query_where_delegates(trait_def);
    let query_order_by_methods = schema_trait_query_order_by_delegates(trait_def);

    let refine_trait_name =
        format_ident!("{}QueryRefine{}", trait_name, to_pascal_case(table_snake));
    let refine_method = format_ident!("where_is_{}", table_snake);
    let table_lit = LitStr::new(table_snake, proc_macro2::Span::call_site());

    let a = schema_trait_named_fields_impl(struct_name, &fields_trait, &field_getters);
    let b = schema_trait_named_query_impl(
        query_name,
        &query_trait,
        &query_where_methods,
        &query_order_by_methods,
    );
    let c = schema_trait_refine_bridge(
        query_name,
        &query_trait,
        &query_all_struct,
        &refine_trait_name,
        &refine_method,
        &table_lit,
    );

    quote! {
        #a
        #b
        #c
    }
}

#[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
pub fn generate_trait_impls(
    schema: &SchemaContext,
    trait_defs: &HashMap<String, ParsedTraitDef>,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let mut tokens = TokenStream::new();
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let query_name = format_ident!("{}Query", struct_name);
    let table_snake = schema.table_name.as_str();

    if !schema.schema.traits.is_empty() {
        tokens.extend(query_builder_parts_bridge_tokens(&query_name));
    }

    for trait_name in &schema.schema.traits {
        let Some(trait_def) = trait_defs.get(trait_name) else {
            continue;
        };
        tokens.extend(schema_trait_impl_bundle(
            &struct_name,
            &query_name,
            table_snake,
            trait_name,
            trait_def,
        ));
    }

    Ok(tokens)
}
