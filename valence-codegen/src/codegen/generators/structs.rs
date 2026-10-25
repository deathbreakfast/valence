//! Main model struct, schema metadata struct, enums, and `IdHolder` wiring.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;

use super::enums::generate_enum_definition;
use super::rust_types::{json_as_helpers_and_attrs, parse_json_as, rust_type_tokens};
use super::validation::generate_validation_code;

#[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
pub fn generate_struct(schema: &SchemaContext) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let model_name = to_pascal_case(&schema.table_name);
    let schema_struct_name = format_ident!("{}Schema", struct_name);
    let table_name = schema.table_name.as_str();

    let mut enum_defs = Vec::new();
    let mut json_as_helpers = Vec::new();
    let mut field_defs = Vec::new();
    let mut getter_methods = Vec::new();
    let mut constructor_params = Vec::new();
    let mut constructor_inits = Vec::new();
    let mut constructor_validations = Vec::new();

    for field in &schema.fields {
        let field_name_str = field.name.as_str();
        let field_name = format_ident!("{}", field_name_str);
        let field_type_str = field.field_type.as_str();
        let is_primary_key = field.primary;
        let is_required = !field.nullable;
        // Required on create, but some viewers read the row without it.
        let read_hidable_required = is_required && field.read_hidable();

        // Generate inline enums for enum: variants.
        if field_type_str.starts_with("enum:")
            && field.enum_type.is_none()
            && !field.enum_variants.is_empty()
        {
            let enum_name = format!("{}{}", model_name, to_pascal_case(field_name_str));
            enum_defs.push(generate_enum_definition(&enum_name, &field.enum_variants));
        }

        let field_type = rust_type_tokens(field, &model_name);

        let mut serde_attrs = quote! {};
        if read_hidable_required {
            // Absent for a viewer the field's read policy denies; never written as null.
            serde_attrs = if field_type_str == "datetime" {
                quote! {
                    #[serde(default, skip_serializing_if = "Option::is_none", with = "valence::datetime_unix::option")]
                }
            } else {
                quote! { #[serde(default, skip_serializing_if = "Option::is_none")] }
            };
        } else if field_type_str == "datetime" {
            serde_attrs = if is_required {
                quote! { #[serde(with = "valence::datetime_unix")] }
            } else {
                quote! { #[serde(default, with = "valence::datetime_unix::option")] }
            };
        } else if let Some((helpers, attrs)) =
            json_as_helpers_and_attrs(field, table_name, &field_type)
        {
            json_as_helpers.push(helpers);
            serde_attrs = attrs;
        } else if field_type_str == "json" && is_required {
            serde_attrs = quote! { #[serde(default)] };
        } else if !is_required {
            serde_attrs = quote! { #[serde(default)] };
        }

        if is_primary_key {
            field_defs.push(quote! {
                #[serde(default)]
                #field_name: Option<valence::RecordId>
            });

            getter_methods.push(quote! {
                pub fn #field_name(&self) -> Option<&valence::RecordId> {
                    self.#field_name.as_ref()
                }
            });
            continue;
        }

        if read_hidable_required {
            let storage_ty = if field_type_str == "datetime" {
                quote! { Option<chrono::DateTime<chrono::Utc>> }
            } else {
                quote! { Option<#field_type> }
            };
            field_defs.push(quote! {
                #serde_attrs
                #field_name: #storage_ty
            });

            getter_methods.push(quote! {
                /// `None` when the viewer that loaded this row can't read the field.
                pub fn #field_name(&self) -> Option<&#field_type> {
                    self.#field_name.as_ref()
                }
            });

            constructor_params.push(quote! { #field_name: #field_type });

            let validation_code = generate_validation_code(field_name_str, &field.validations);
            if !validation_code.is_empty() {
                constructor_validations.push(validation_code);
            }

            constructor_inits.push(quote! { #field_name: Some(#field_name) });
        } else if is_required {
            field_defs.push(quote! {
                #serde_attrs
                #field_name: #field_type
            });

            getter_methods.push(quote! {
                pub fn #field_name(&self) -> &#field_type {
                    &self.#field_name
                }
            });

            constructor_params.push(quote! { #field_name: #field_type });

            let validation_code = generate_validation_code(field_name_str, &field.validations);
            if !validation_code.is_empty() {
                constructor_validations.push(validation_code);
            }

            constructor_inits.push(quote! { #field_name });
        } else {
            // Optional datetime / JsonAs: Option wrapping.
            let storage_ty = if field_type_str == "datetime" {
                quote! { Option<chrono::DateTime<chrono::Utc>> }
            } else if parse_json_as(field_type_str).is_some() {
                quote! { Option<#field_type> }
            } else {
                quote! { Option<#field_type> }
            };

            field_defs.push(quote! {
                #serde_attrs
                #field_name: #storage_ty
            });

            getter_methods.push(quote! {
                pub fn #field_name(&self) -> Option<&#field_type> {
                    self.#field_name.as_ref()
                }
            });

            constructor_params.push(quote! { #field_name: Option<#field_type> });
            constructor_inits.push(quote! { #field_name });
        }
    }

    let reference_name = format_ident!("{}Reference", struct_name);

    Ok(quote! {
        #(#enum_defs)*

        #(#json_as_helpers)*

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct #struct_name {
            #(#field_defs),*
        }

        /// Reference to a #struct_name for batch creation
        #[allow(dead_code)]
        pub type #reference_name = valence::Reference<#struct_name>;

        #[allow(dead_code)]
        impl #struct_name {
            /// Create a new #struct_name
            pub fn new(#(#constructor_params),*) -> valence::Result<Self> {
                #(#constructor_validations)*

                Ok(Self {
                    id: None,
                    #(#constructor_inits),*
                })
            }

            /// Get the full static schema reference
            pub fn get_schema() -> &'static valence::Schema {
                #schema_struct_name::full()
            }

            #(#getter_methods)*
        }

        impl valence::WithReference for #struct_name {}
    })
}
