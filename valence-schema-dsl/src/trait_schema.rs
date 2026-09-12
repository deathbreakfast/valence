//! Syn parsers for `valence_trait_schema!`.
//!
//! # Grammar (inside `TraitName { ... }`)
//!
//! - **`fields`** — same field attribute grammar as [`crate::FieldsConfig`]
//! - **`connections`** — full connection attrs (codegen merges them; macros emit names only)
//! - **`policies`** — same shape as schema-level `policies:`

use syn::punctuated::Punctuated;
use syn::{
    braced,
    parse::{Parse, ParseStream},
    token, Ident, Result, Token,
};

use crate::parse::{
    parse_connections, parse_fields_public, parse_policies_config, ConnectionsConfig, FieldsConfig,
    ParsedConnection, ParsedField, ParsedPolicies, PoliciesConfig, RepositoryConfig,
};

/// Lowered trait DSL consumed by macros and codegen.
#[derive(Debug, Clone)]
pub struct ParsedTraitSchema {
    pub name: String,
    /// Canonical Git repository URL (required).
    pub repository: String,
    pub fields: Vec<ParsedField>,
    /// Full connection metadata (codegen trait merge).
    pub connections: Vec<ParsedConnection>,
    /// Entity-level privacy policies declared by this trait.
    pub policies: Option<ParsedPolicies>,
}

impl ParsedTraitSchema {
    /// Connection names for macro `TraitDefinition` registration.
    pub fn connection_names(&self) -> Vec<String> {
        self.connections.iter().map(|c| c.name.clone()).collect()
    }
}

/// Top-level `TraitName { fields: ..., connections: ..., policies: ... }` input.
pub struct TraitSchemaSpec {
    pub name: Ident,
    _brace: token::Brace,
    pub items: Punctuated<TraitSchemaItem, Token![,]>,
}

impl Parse for TraitSchemaSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;
        let content;
        let brace = braced!(content in input);
        let items = content.parse_terminated(TraitSchemaItem::parse, Token![,])?;
        Ok(TraitSchemaSpec {
            name,
            _brace: brace,
            items,
        })
    }
}

pub enum TraitSchemaItem {
    Fields(FieldsConfig),
    Connections(ConnectionsConfig),
    Policies(PoliciesConfig),
    Repository(RepositoryConfig),
}

impl Parse for TraitSchemaItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        match key.to_string().as_str() {
            "fields" => Ok(TraitSchemaItem::Fields(input.parse()?)),
            "connections" => Ok(TraitSchemaItem::Connections(input.parse()?)),
            "policies" => Ok(TraitSchemaItem::Policies(input.parse()?)),
            "repository" => Ok(TraitSchemaItem::Repository(input.parse()?)),
            _ => Err(syn::Error::new(
                key.span(),
                format!(
                    "Unknown trait schema key: {key} (expected `fields`, `connections`, `policies`, or `repository`)"
                ),
            )),
        }
    }
}

impl TraitSchemaSpec {
    /// Lower parsed items into a [`ParsedTraitSchema`].
    pub fn to_parsed(&self) -> Result<ParsedTraitSchema> {
        let mut fields = Vec::new();
        let mut connections = Vec::new();
        let mut policies = None;
        let mut repository = None;

        for item in &self.items {
            match item {
                TraitSchemaItem::Fields(f) => {
                    fields = parse_fields_public(f)?;
                }
                TraitSchemaItem::Connections(c) => {
                    connections = parse_connections(c)?;
                }
                TraitSchemaItem::Policies(p) => {
                    policies = Some(parse_policies_config(p)?);
                }
                TraitSchemaItem::Repository(r) => {
                    if repository.is_some() {
                        return Err(syn::Error::new(
                            self.name.span(),
                            "duplicate `repository:` in valence_trait_schema!",
                        ));
                    }
                    repository = Some(r.value.value());
                }
            }
        }

        let repository = repository.ok_or_else(|| {
            syn::Error::new(
                self.name.span(),
                "Missing required `repository: \"…\"` in valence_trait_schema!",
            )
        })?;
        if repository.trim().is_empty() {
            return Err(syn::Error::new(
                self.name.span(),
                "`repository` must be a non-empty Git URL",
            ));
        }

        Ok(ParsedTraitSchema {
            name: self.name.to_string(),
            repository,
            fields,
            connections,
            policies,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_trait() {
        let input = r#"
            Named {
                repository: "https://github.com/unified-field-dev/valence",
                fields: [
                    name: { r#type: FieldType::String, required: true },
                ],
            }
        "#;
        let spec = syn::parse_str::<TraitSchemaSpec>(input).expect("parse");
        let parsed = spec.to_parsed().expect("to_parsed");
        assert_eq!(parsed.name, "Named");
        assert_eq!(parsed.fields.len(), 1);
        assert_eq!(parsed.fields[0].name, "name");
        assert_eq!(parsed.fields[0].field_type, "string");
        assert!(parsed.fields[0].required);
        assert!(parsed.connections.is_empty());
        assert!(parsed.policies.is_none());
    }

    #[test]
    fn test_parse_trait_with_connections() {
        let input = r#"
            HasFiles {
                repository: "https://github.com/unified-field-dev/valence",
                fields: [],
                connections: [
                    files: { table: "file", cardinality: HasMany, reverse_field: "parent", on_delete: Cascade },
                ],
            }
        "#;
        let spec = syn::parse_str::<TraitSchemaSpec>(input).expect("parse");
        let parsed = spec.to_parsed().expect("to_parsed");
        assert_eq!(parsed.name, "HasFiles");
        assert!(parsed.fields.is_empty());
        assert_eq!(parsed.connection_names(), vec!["files"]);
        assert_eq!(parsed.connections[0].table, "file");
        assert_eq!(parsed.connections[0].cardinality, "HasMany");
    }

    #[test]
    fn test_parse_trait_with_fields_and_connections() {
        let input = r#"
            HasOwner {
                repository: "https://github.com/unified-field-dev/valence",
                fields: [
                    owner: { r#type: FieldType::Record("user"), required: true },
                ],
                connections: [
                    owner: { table: "user", cardinality: HasOne, required: true, on_delete: Cascade, model: "crate::generated::User" },
                ],
            }
        "#;
        let spec = syn::parse_str::<TraitSchemaSpec>(input).expect("parse");
        let parsed = spec.to_parsed().expect("to_parsed");
        assert_eq!(parsed.name, "HasOwner");
        assert_eq!(parsed.fields.len(), 1);
        assert_eq!(parsed.fields[0].name, "owner");
        assert_eq!(parsed.connection_names(), vec!["owner"]);
        assert_eq!(
            parsed.connections[0].model.as_deref(),
            Some("crate::generated::User")
        );
    }

    #[test]
    fn test_parse_trait_with_policies() {
        let input = r#"
            Secured {
                repository: "https://github.com/unified-field-dev/valence",
                fields: [
                    name: { r#type: FieldType::String, required: true },
                ],
                policies: {
                    read: { allow: [PUBLIC_READ] },
                    create: { allow: [AUTHENTICATED], block: [BLOCK_ALL] },
                },
            }
        "#;
        let spec = syn::parse_str::<TraitSchemaSpec>(input).expect("parse");
        let parsed = spec.to_parsed().expect("to_parsed");
        assert_eq!(parsed.name, "Secured");
        assert_eq!(parsed.fields.len(), 1);
        let policies = parsed.policies.expect("should have policies");
        assert!(policies.read.is_some());
        assert!(policies.create.is_some());
        assert!(policies.update.is_none());
        assert!(policies.delete.is_none());

        let read = policies.read.unwrap();
        assert_eq!(read.allow.len(), 1);
        assert!(read.block.is_empty());

        let create = policies.create.unwrap();
        assert_eq!(create.allow.len(), 1);
        assert_eq!(create.block.len(), 1);
    }

    #[test]
    fn test_parse_trait_with_fields_connections_and_policies() {
        let input = r#"
            HasOwnerSecured {
                repository: "https://github.com/unified-field-dev/valence",
                fields: [
                    owner: { r#type: FieldType::Record("user"), required: true },
                ],
                connections: [
                    owner: { table: "user", cardinality: HasOne, required: true, on_delete: Cascade },
                ],
                policies: {
                    read: { allow: [AUTHENTICATED] },
                },
            }
        "#;
        let spec = syn::parse_str::<TraitSchemaSpec>(input).expect("parse");
        let parsed = spec.to_parsed().expect("to_parsed");
        assert_eq!(parsed.name, "HasOwnerSecured");
        assert_eq!(parsed.fields.len(), 1);
        assert_eq!(parsed.connection_names(), vec!["owner"]);
        assert!(parsed.policies.is_some());
    }
}
