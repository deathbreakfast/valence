use proc_macro2::TokenStream;
use syn::punctuated::Punctuated;
use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    token, Ident, LitBool, Result, Token,
};

use super::privacy::{parse_policies_config, ParsedPolicies, PoliciesConfig};

#[derive(Debug, Clone)]
pub struct ParsedField {
    pub name: String,
    pub field_type: String,
    /// Explicit model path from `Record(…).target(…)` for connection/query codegen.
    pub model_path: Option<String>,
    pub required: bool,
    pub primary_key: bool,
    pub unique: bool,
    /// Ask the backend for a non-unique lookup index on this field.
    pub indexed: bool,
    pub default: Option<String>,
    pub validations: Vec<String>,
    pub policies: Option<ParsedPolicies>,
    pub encrypted: bool,
}

pub struct FieldsConfig {
    _bracket: token::Bracket,
    pub fields: Punctuated<FieldSpec, Token![,]>,
}

impl Parse for FieldsConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(FieldsConfig {
            _bracket: bracketed!(content in input),
            fields: content.parse_terminated(FieldSpec::parse, Token![,])?,
        })
    }
}

pub struct FieldSpec {
    pub name: Ident,
    _colon: Token![:],
    _brace: token::Brace,
    pub attrs: Punctuated<FieldAttr, Token![,]>,
}

impl Parse for FieldSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(FieldSpec {
            name: input.parse()?,
            _colon: input.parse()?,
            _brace: braced!(content in input),
            attrs: content.parse_terminated(FieldAttr::parse, Token![,])?,
        })
    }
}

pub enum FieldAttr {
    Type(TokenStream),
    Required(LitBool),
    PrimaryKey(LitBool),
    Unique(LitBool),
    Indexed(LitBool),
    Default(TokenStream),
    Validations(ValidatorList),
    Policies(PoliciesConfig),
    Encrypted(LitBool),
}

impl Parse for FieldAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        match key.to_string().as_str() {
            "r#type" | "type" => {
                let tokens = parse_until_comma_or_end(input)?;
                Ok(FieldAttr::Type(tokens))
            }
            "required" => Ok(FieldAttr::Required(input.parse()?)),
            "primary_key" => Ok(FieldAttr::PrimaryKey(input.parse()?)),
            "unique" => Ok(FieldAttr::Unique(input.parse()?)),
            "indexed" => Ok(FieldAttr::Indexed(input.parse()?)),
            "default" => {
                let tokens = parse_until_comma_or_end(input)?;
                Ok(FieldAttr::Default(tokens))
            }
            "validations" => Ok(FieldAttr::Validations(input.parse()?)),
            "policies" => Ok(FieldAttr::Policies(input.parse()?)),
            "encrypted" => Ok(FieldAttr::Encrypted(input.parse()?)),
            _ => Err(syn::Error::new(
                key.span(),
                format!("Unknown field attribute: {key}"),
            )),
        }
    }
}

fn parse_until_comma_or_end(input: ParseStream) -> Result<TokenStream> {
    use proc_macro2::TokenTree;
    use quote::TokenStreamExt;

    let mut tokens = TokenStream::new();

    while !input.is_empty() && !input.peek(Token![,]) {
        let tt: TokenTree = input.parse()?;
        tokens.append(tt);
    }

    Ok(tokens)
}

pub struct ValidatorList {
    _bracket: token::Bracket,
    pub validators: Punctuated<TokenStream, Token![,]>,
}

impl Parse for ValidatorList {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        let bracket = bracketed!(content in input);
        let mut validators = Punctuated::new();

        while !content.is_empty() {
            let tokens: TokenStream = content.parse()?;
            validators.push_value(tokens);

            if !content.is_empty() {
                validators.push_punct(content.parse()?);
            }
        }

        Ok(ValidatorList {
            _bracket: bracket,
            validators,
        })
    }
}

/// Public entry point for reuse by the trait parser.
pub fn parse_fields_public(config: &FieldsConfig) -> Result<Vec<ParsedField>> {
    parse_fields(config)
}

pub(super) fn parse_fields(config: &FieldsConfig) -> Result<Vec<ParsedField>> {
    let mut fields = Vec::new();

    for field in &config.fields {
        let mut field_type = None;
        let mut model_path = None;
        let mut required = true;
        let mut primary_key = false;
        let mut unique = false;
        let mut indexed = false;
        let mut default = None;
        let mut validations = Vec::new();
        let mut policies = None;
        let mut encrypted = false;

        for attr in &field.attrs {
            match attr {
                FieldAttr::Type(t) => {
                    let extracted = crate::extract::extract_field_type(t)?;
                    field_type = Some(extracted.field_type);
                    model_path = extracted.model_path;
                }
                FieldAttr::Required(b) => required = b.value,
                FieldAttr::PrimaryKey(b) => primary_key = b.value,
                FieldAttr::Unique(b) => unique = b.value,
                FieldAttr::Indexed(b) => indexed = b.value,
                FieldAttr::Default(d) => default = Some(crate::extract::extract_default_string(d)),
                FieldAttr::Validations(v) => {
                    validations = v
                        .validators
                        .iter()
                        .map(crate::extract::extract_validator_string)
                        .collect::<Result<Vec<_>>>()?;
                }
                FieldAttr::Policies(p) => policies = Some(parse_policies_config(p)?),
                FieldAttr::Encrypted(b) => encrypted = b.value,
            }
        }

        let field_type = field_type.ok_or_else(|| {
            syn::Error::new(
                field.name.span(),
                format!("Field '{}' missing type", field.name),
            )
        })?;

        fields.push(ParsedField {
            name: field.name.to_string(),
            field_type,
            model_path,
            required,
            primary_key,
            unique,
            indexed,
            default,
            validations,
            policies,
            encrypted,
        });
    }

    Ok(fields)
}
