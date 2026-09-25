//! Per-field `where_*`, `order_by_*`, `distinct_*`, and nullability helpers on the query builder.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::LitStr;

use crate::codegen::schema::SchemaContext;

pub(super) fn collect_field_query_methods(
    schema: &SchemaContext,
) -> (Vec<TokenStream>, Vec<TokenStream>, Vec<TokenStream>) {
    let mut where_methods = Vec::new();
    let mut order_by_methods = Vec::new();
    let mut distinct_methods = Vec::new();

    for field in &schema.fields {
        let field_name_str = field.name.as_str();
        let where_method_name = format_ident!("where_{}", field_name_str);
        let order_by_method_name = format_ident!("order_by_{}", field_name_str);
        let field_type_str = field.field_type.as_str();
        let field_name_lit = LitStr::new(field_name_str, proc_macro2::Span::call_site());

        let where_method = if field_type_str.starts_with("record<") && field_type_str.ends_with('>')
        {
            quote! {
                pub fn #where_method_name(mut self, predicate: valence::RecordPredicate) -> Self {
                    self.inner = self.inner.where_record(#field_name_lit.to_string(), predicate);
                    self
                }
            }
        } else if field_type_str == "currency" {
            let where_code = format_ident!("where_{}_code", field_name_str);
            let where_minor = format_ident!("where_{}_minor", field_name_str);
            let code_path = format!("{field_name_str}.code");
            let minor_path = format!("{field_name_str}.amount_minor");
            let code_path_lit = LitStr::new(&code_path, proc_macro2::Span::call_site());
            let minor_path_lit = LitStr::new(&minor_path, proc_macro2::Span::call_site());
            quote! {
                /// Filter by currency code (alphabetic ISO string on the wire).
                pub fn #where_code(mut self, code: valence::CurrencyCode) -> Self {
                    self.inner = self.inner.where_string(
                        #code_path_lit.to_string(),
                        valence::StringPredicate::Equals(code.as_str().to_string()),
                    );
                    self
                }

                /// Filter by minor-unit amount.
                pub fn #where_minor(mut self, predicate: valence::IntPredicate) -> Self {
                    self.inner = self.inner.where_int(#minor_path_lit.to_string(), predicate);
                    self
                }
            }
        } else if field_type_str.starts_with("json_as:") {
            // Opaque typed JSON: no whole-field string predicate.
            quote! {}
        } else {
            match field_type_str {
                "integer" => quote! {
                    pub fn #where_method_name(mut self, predicate: valence::IntPredicate) -> Self {
                        self.inner = self.inner.where_int(#field_name_lit.to_string(), predicate);
                        self
                    }
                },
                "string" | "text" => quote! {
                    pub fn #where_method_name(mut self, predicate: valence::StringPredicate) -> Self {
                        self.inner = self.inner.where_string(#field_name_lit.to_string(), predicate);
                        self
                    }
                },
                "datetime" => quote! {
                    pub fn #where_method_name(mut self, predicate: valence::DateTimePredicate) -> Self {
                        self.inner = self.inner.where_datetime(#field_name_lit.to_string(), predicate);
                        self
                    }
                },
                "json" => quote! {},
                _ => quote! {
                    pub fn #where_method_name(mut self, predicate: valence::StringPredicate) -> Self {
                        self.inner = self.inner.where_string(#field_name_lit.to_string(), predicate);
                        self
                    }
                },
            }
        };
        if !where_method.is_empty() {
            where_methods.push(where_method);
        }

        if field.nullable {
            let where_is_none_method = format_ident!("where_{}_is_none", field_name_str);
            let where_is_some_method = format_ident!("where_{}_is_some", field_name_str);
            let field_name_lit = LitStr::new(field_name_str, proc_macro2::Span::call_site());

            where_methods.push(quote! {
                /// Filter for records where this field is NULL
                pub fn #where_is_none_method(mut self) -> Self {
                    self.inner = self.inner.where_null(#field_name_lit.to_string(), valence::NullPredicate::IsNone);
                    self
                }

                /// Filter for records where this field is NOT NULL
                pub fn #where_is_some_method(mut self) -> Self {
                    self.inner = self.inner.where_null(#field_name_lit.to_string(), valence::NullPredicate::IsSome);
                    self
                }
            });
        }

        let field_name_lit = LitStr::new(field_name_str, proc_macro2::Span::call_site());
        order_by_methods.push(quote! {
            pub fn #order_by_method_name(mut self, direction: valence::SortDirection) -> Self {
                self.inner = self.inner.order_by(#field_name_lit.to_string(), direction);
                self
            }
        });

        let is_record = field_type_str.starts_with("record<") && field_type_str.ends_with('>');
        let is_string_like = matches!(field_type_str, "string" | "text")
            || (!is_record
                && !field_type_str.starts_with("json_as:")
                && !matches!(
                    field_type_str,
                    "integer" | "float" | "decimal" | "boolean" | "datetime" | "json" | "currency"
                ));
        if is_string_like {
            let distinct_method_name = format_ident!("distinct_{}", field_name_str);
            let field_name_lit = LitStr::new(field_name_str, proc_macro2::Span::call_site());
            distinct_methods.push(quote! {
                pub async fn #distinct_method_name(
                    self,
                    purpose: valence::DataUsePurpose,
                ) -> valence::Result<Vec<String>> {
                    self.inner
                        .distinct_values(#field_name_lit, self.valence, purpose)
                        .await
                }
            });
        }
    }

    (where_methods, order_by_methods, distinct_methods)
}
