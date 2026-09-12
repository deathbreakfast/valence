//! Type-safe `*Query` builder: field predicates, connection filters, hops, composite WHERE.

mod composite_where;
mod connection_methods;
mod field_methods;
mod target;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;

use self::composite_where::generate_composite_key_where;
use self::connection_methods::collect_connection_hop_methods;
use self::field_methods::collect_field_query_methods;

/// Generate `Model::query`, the `*Query` struct, `IntoFuture`, and all builder methods.
pub fn generate_query_builder(
    schema: &SchemaContext,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let query_name = format_ident!("{}Query", struct_name);
    let table_name_lit = schema.table_name.as_str();

    let (where_methods, order_by_methods, distinct_methods) = collect_field_query_methods(schema);
    let (connection_methods, hop_methods) = collect_connection_hop_methods(schema);
    let composite_key_where_method = generate_composite_key_where(schema)?;

    Ok(quote! {
        impl #struct_name {
            /// Start a type-safe query
            #[deprecated(note = "use query_used with use_!(...) for declared data-use transparency")]
            pub fn query(valence: &valence::Valence) -> #query_name<'_> {
                #query_name {
                    inner: valence::QueryCore::new(#table_name_lit.to_string()),
                    valence,
                }
            }

            /// Declared schema query builder (catalog target: Schema).
            pub fn query_used(
                valence: &valence::Valence,
                purpose: valence::DataUsePurpose,
            ) -> #query_name<'_> {
                let _ = purpose;
                #[allow(deprecated)]
                Self::query(valence)
            }
        }

        /// Query builder for #struct_name
        #[derive(Clone)]
        #[allow(dead_code)]
        pub struct #query_name<'a> {
            pub inner: valence::QueryCore,
            valence: &'a valence::Valence,
        }

        #[allow(dead_code)]
        impl<'a> #query_name<'a> {
            /// Build a query from an already-configured [`valence::QueryCore`].
            ///
            /// Used by cross-crate connection hops; the `valence` field is otherwise private.
            pub fn from_parts(inner: valence::QueryCore, valence: &'a valence::Valence) -> Self {
                Self { inner, valence }
            }

            /// Split into the underlying query core and valence handle.
            pub fn into_parts(self) -> (valence::QueryCore, &'a valence::Valence) {
                (self.inner, self.valence)
            }

            #(#where_methods)*

            #(#connection_methods)*

            #(#hop_methods)*

            #(#order_by_methods)*

            #(#distinct_methods)*

            #composite_key_where_method

            /// OR-combine another query's conditions with this one.
            /// Returns rows matching *either* set of WHERE conditions.
            pub fn union(mut self, other: Self) -> Self {
                self.inner = self.inner.union_with(other.inner);
                self
            }

            /// AND-combine another query's conditions with this one.
            /// Returns rows matching *both* sets of WHERE conditions.
            pub fn join(mut self, other: Self) -> Self {
                self.inner = self.inner.join_with(other.inner);
                self
            }

            /// Limit the number of returned rows
            pub fn limit(mut self, limit: u32) -> Self {
                self.inner = self.inner.limit(limit);
                self
            }

            /// Skip the first `offset` rows
            pub fn offset(mut self, offset: u32) -> Self {
                self.inner = self.inner.offset(offset);
                self
            }

            /// Execute the query and return the first result, or `None`
            pub async fn first(self) -> valence::Result<Option<#struct_name>> {
                let mut results = self.limit(1).await?;
                Ok(results.pop())
            }
        }

        impl<'a> std::future::IntoFuture for #query_name<'a> {
            type Output = valence::Result<Vec<#struct_name>>;
            type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

            fn into_future(self) -> Self::IntoFuture {
                Box::pin(async move {
                    self.inner.execute(self.valence).await
                })
            }
        }
    })
}
