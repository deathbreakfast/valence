//! Token fragments for the mutable builder (`set_*`, `clear_*`, `*_has_change`, `commit_updates`).

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::schema::SchemaContext;

use super::super::validation::generate_validation_code;
use super::field_tokens::field_type_tokens_for;

pub(super) struct MutablePartStreams {
    pub mutable_fields: Vec<TokenStream>,
    pub mutable_field_names: Vec<proc_macro2::Ident>,
    pub setter_methods: Vec<TokenStream>,
    pub clear_methods: Vec<TokenStream>,
    pub has_change_methods: Vec<TokenStream>,
    pub commit_updates: Vec<TokenStream>,
}

pub(super) fn collect_mutable_part_streams(schema: &SchemaContext) -> MutablePartStreams {
    let model_name = crate::codegen::utils::to_pascal_case(&schema.table_name);

    let mut mutable_fields = Vec::new();
    let mut mutable_field_names = Vec::new();
    let mut setter_methods = Vec::new();
    let mut clear_methods = Vec::new();
    let mut has_change_methods = Vec::new();
    let mut commit_updates = Vec::new();

    for field in &schema.fields {
        let field_name_str = field.name.as_str();
        let is_primary_key = field.primary;
        let is_composite_key_field = schema.composite_key.contains(&field.name);

        if is_primary_key || is_composite_key_field {
            continue;
        }

        let field_name = format_ident!("{}", field_name_str);
        let setter_name = format_ident!("set_{}", field_name_str);
        let has_change_name = format_ident!("{}_has_change", field_name_str);

        mutable_field_names.push(field_name.clone());

        let field_type = field_type_tokens_for(field, &model_name);

        let is_required = !field.nullable;

        let validation_code = generate_validation_code("value", &field.validations);

        if is_required {
            mutable_fields.push(quote! {
                #field_name: Option<#field_type>
            });

            setter_methods.push(quote! {
                pub fn #setter_name(mut self, value: #field_type) -> valence::Result<Self> {
                    #validation_code
                    self.#field_name = Some(value);
                    Ok(self)
                }
            });

            commit_updates.push(quote! {
                if let Some(val) = self.#field_name.take() {
                    self.model.#field_name = val;
                }
            });
        } else {
            mutable_fields.push(quote! {
                #field_name: Option<Option<#field_type>>
            });

            setter_methods.push(quote! {
                pub fn #setter_name(mut self, value: #field_type) -> valence::Result<Self> {
                    #validation_code
                    self.#field_name = Some(Some(value));
                    Ok(self)
                }
            });

            let clear_name = format_ident!("clear_{}", field_name_str);
            clear_methods.push(quote! {
                pub fn #clear_name(mut self) -> Self {
                    self.#field_name = Some(None);
                    self
                }
            });

            commit_updates.push(quote! {
                if let Some(opt) = self.#field_name.take() {
                    self.model.#field_name = opt;
                }
            });
        }

        has_change_methods.push(quote! {
            pub fn #has_change_name(&self) -> bool {
                self.#field_name.is_some()
            }
        });
    }

    MutablePartStreams {
        mutable_fields,
        mutable_field_names,
        setter_methods,
        clear_methods,
        has_change_methods,
        commit_updates,
    }
}
