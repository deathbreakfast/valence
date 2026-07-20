//! Main model struct, schema metadata struct, enums, and `IdHolder` wiring.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::schema::SchemaContext;
use crate::codegen::utils::to_pascal_case;
use valence_core::SchemaField;

use super::enums::generate_enum_definition;
use super::validation::generate_validation_code;

fn scalar_field_type_tokens(field_type_str: &str) -> TokenStream {
    match field_type_str {
        "string" => quote! { String },
        "integer" => quote! { i64 },
        "float" => quote! { f64 },
        "boolean" => quote! { bool },
        "datetime" => quote! { chrono::DateTime<chrono::Utc> },
        "json" => quote! { serde_json::Value },
        _ => quote! { String },
    }
}

/// Rust type for a model field; may push a generated enum definition into `enum_defs`.
fn struct_field_rust_type_tokens(
    field: &SchemaField,
    model_name: &str,
    enum_defs: &mut Vec<TokenStream>,
) -> TokenStream {
    let field_type_str = field.field_type.as_str();
    let field_name_str = field.name.as_str();

    if field_type_str.starts_with("enum:") || field_type_str.starts_with("ext_enum:") {
        if let Some(ref etype) = field.enum_type {
            return etype.parse().unwrap_or_else(|_| {
                let ident = format_ident!("{}", etype);
                quote! { #ident }
            });
        }
        if field_type_str.starts_with("enum:") && !field.enum_variants.is_empty() {
            let enum_name = format!("{}{}", model_name, to_pascal_case(field_name_str));
            enum_defs.push(generate_enum_definition(&enum_name, &field.enum_variants));
            let ident = format_ident!("{}", enum_name);
            return quote! { #ident };
        }
        return quote! { String };
    }

    if field_type_str.starts_with("record<") && field_type_str.ends_with('>') {
        return quote! { valence::RecordId };
    }

    scalar_field_type_tokens(field_type_str)
}

#[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
pub fn generate_struct(schema: &SchemaContext) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let struct_name = format_ident!("{}", to_pascal_case(&schema.table_name));
    let model_name = to_pascal_case(&schema.table_name);
    let schema_struct_name = format_ident!("{}Schema", struct_name);

    let mut enum_defs = Vec::new();
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

        let field_type: TokenStream =
            struct_field_rust_type_tokens(field, &model_name, &mut enum_defs);

        let is_json_type = field_type_str == "json";

        // For primary key (id), it's optional
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
        } else {
            // Regular fields
            if is_required {
                if is_json_type {
                    field_defs.push(quote! {
                        #[serde(default)]
                        #field_name: #field_type
                    });
                } else {
                    field_defs.push(quote! {
                        #field_name: #field_type
                    });
                }

                getter_methods.push(quote! {
                    pub fn #field_name(&self) -> &#field_type {
                        &self.#field_name
                    }
                });

                // Add to constructor parameters
                constructor_params.push(quote! { #field_name: #field_type });

                // Generate validation code if validations exist
                let validation_code = generate_validation_code(field_name_str, &field.validations);
                if !validation_code.is_empty() {
                    constructor_validations.push(validation_code);
                }

                constructor_inits.push(quote! { #field_name });
            } else {
                field_defs.push(quote! {
                    #[serde(default)]
                    #field_name: Option<#field_type>
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
    }

    // Generate Reference type alias
    let reference_name = format_ident!("{}Reference", struct_name);

    Ok(quote! {
        #(#enum_defs)*

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
                // Validations
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
