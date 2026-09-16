//! Top-level `Name { table, fields, ... }` grammar for `valence_schema!`.

use syn::punctuated::Punctuated;
use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    token, Expr, Ident, LitStr, Result, Token,
};

use super::connections::{parse_connections, ConnectionsConfig, ParsedConnection};
use super::fields::{parse_fields, FieldsConfig, ParsedField};
use super::ownership::{parse_ownership_block, OwnershipConfigBlock, ParsedOwnershipConfig};
use super::privacy::{parse_policies_config, ParsedPolicies, PoliciesConfig, PrivacyConfig};
use super::ttl::{parse_ttl_config, ParsedTtlPolicy, TtlConfig};

/// Root schema definition.
pub struct SchemaSpec {
    pub name: Ident,
    _brace: token::Brace,
    pub items: Punctuated<SchemaItem, Token![,]>,
}

/// Lowered schema DSL consumed by macros and codegen.
#[derive(Debug, Clone)]
pub struct ParsedSchema {
    pub table_name: String,
    pub version: String,
    pub description: Option<String>,
    /// Canonical Git repository URL (required).
    pub repository: String,
    /// Retention label from `meta:` (default `"365 days"` when omitted).
    pub retention: String,
    /// Owner label from `meta:` (default `"system"` when omitted).
    pub owner: String,
    /// Optional `database:` — any Rust expression of type `&'static dyn DatabaseEvaluator`.
    pub database: Option<Expr>,
    pub ttl: Option<ParsedTtlPolicy>,
    pub policies: Option<ParsedPolicies>,
    pub fields: Vec<ParsedField>,
    pub connections: Vec<ParsedConnection>,
    pub side_effects: Vec<String>,
    /// `iters: [MyIter, ...]` — row-level hooks.
    pub iters: Vec<String>,
    pub composite_key: Vec<String>,
    pub traits: Vec<String>,
    pub ownership: Option<ParsedOwnershipConfig>,
}

impl Parse for SchemaSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;
        let content;
        let brace = braced!(content in input);
        let items = content.parse_terminated(SchemaItem::parse, Token![,])?;

        Ok(SchemaSpec {
            name,
            _brace: brace,
            items,
        })
    }
}

/// Top-level key inside `Name { ... }` for `valence_schema!`.
pub enum SchemaItem {
    Table(TableConfig),
    Version(VersionConfig),
    Description(DescriptionConfig),
    /// Top-level `repository: "https://…"`.
    Repository(RepositoryConfig),
    /// Nested `meta: { repository, retention?, owner?, description? }`.
    Meta(MetaConfig),
    Ttl(TtlConfig),
    /// Legacy `privacy:` block (parsed, ignored when building [`ParsedSchema`]).
    #[allow(dead_code)]
    Privacy(PrivacyConfig),
    Policies(PoliciesConfig),
    Fields(FieldsConfig),
    Connections(ConnectionsConfig),
    SideEffects(SideEffectsConfig),
    /// `iters: [IterType1, IterType2]` — same shape as side effects list.
    Iters(SideEffectsConfig),
    CompositeKey(CompositeKeyConfig),
    Traits(TraitsConfig),
    Database(DatabaseConfig),
    Ownership(OwnershipConfigBlock),
}

/// Traits list: `traits: [Named, HasFiles]`
pub struct TraitsConfig {
    _bracket: token::Bracket,
    pub traits: Punctuated<Ident, Token![,]>,
}

impl Parse for TraitsConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(TraitsConfig {
            _bracket: bracketed!(content in input),
            traits: content.parse_terminated(Ident::parse, Token![,])?,
        })
    }
}

impl Parse for SchemaItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        match key.to_string().as_str() {
            "table" => Ok(SchemaItem::Table(input.parse()?)),
            "version" => Ok(SchemaItem::Version(input.parse()?)),
            "description" => Ok(SchemaItem::Description(input.parse()?)),
            "repository" => Ok(SchemaItem::Repository(input.parse()?)),
            "meta" => Ok(SchemaItem::Meta(input.parse()?)),
            "ttl" => Ok(SchemaItem::Ttl(input.parse()?)),
            "privacy" => Ok(SchemaItem::Privacy(input.parse()?)),
            "policies" => Ok(SchemaItem::Policies(input.parse()?)),
            "fields" => Ok(SchemaItem::Fields(input.parse()?)),
            "connections" => Ok(SchemaItem::Connections(input.parse()?)),
            "side_effects" => Ok(SchemaItem::SideEffects(input.parse()?)),
            "iters" => Ok(SchemaItem::Iters(input.parse()?)),
            "composite_key" => Ok(SchemaItem::CompositeKey(input.parse()?)),
            "traits" => Ok(SchemaItem::Traits(input.parse()?)),
            "database" => Ok(SchemaItem::Database(input.parse()?)),
            "ownership" => Ok(SchemaItem::Ownership(input.parse()?)),
            _ => Err(syn::Error::new(
                key.span(),
                format!("Unknown schema key: {key}"),
            )),
        }
    }
}

/// Parsed `database:` value: a Rust expression (path to a `const` / `static` database evaluator).
pub struct DatabaseConfig {
    pub expr: Expr,
}

impl Parse for DatabaseConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(DatabaseConfig {
            expr: input.parse()?,
        })
    }
}

pub struct TableConfig {
    pub value: LitStr,
}

impl Parse for TableConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(TableConfig {
            value: input.parse()?,
        })
    }
}

pub struct VersionConfig {
    pub value: LitStr,
}

impl Parse for VersionConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(VersionConfig {
            value: input.parse()?,
        })
    }
}

pub struct DescriptionConfig {
    pub value: LitStr,
}

impl Parse for DescriptionConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(DescriptionConfig {
            value: input.parse()?,
        })
    }
}

pub struct RepositoryConfig {
    pub value: LitStr,
}

impl Parse for RepositoryConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(RepositoryConfig {
            value: input.parse()?,
        })
    }
}

/// Nested `meta: { repository: "…", retention?: "…", owner?: "…", description?: "…" }`.
pub struct MetaConfig {
    _brace: token::Brace,
    pub items: Punctuated<MetaItem, Token![,]>,
}

pub enum MetaItem {
    Repository(LitStr),
    Retention(LitStr),
    Owner(LitStr),
    Description(LitStr),
}

impl Parse for MetaItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let value: LitStr = input.parse()?;
        match key.to_string().as_str() {
            "repository" => Ok(MetaItem::Repository(value)),
            "retention" => Ok(MetaItem::Retention(value)),
            "owner" => Ok(MetaItem::Owner(value)),
            "description" => Ok(MetaItem::Description(value)),
            _ => Err(syn::Error::new(
                key.span(),
                format!(
                    "Unknown meta key: {key} (expected `repository`, `retention`, `owner`, or `description`)"
                ),
            )),
        }
    }
}

impl Parse for MetaConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(MetaConfig {
            _brace: braced!(content in input),
            items: content.parse_terminated(MetaItem::parse, Token![,])?,
        })
    }
}

/// Side effects configuration: `side_effects: [TypeName1, TypeName2]`
pub struct SideEffectsConfig {
    _bracket: token::Bracket,
    pub effects: Punctuated<Ident, Token![,]>,
}

impl Parse for SideEffectsConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(SideEffectsConfig {
            _bracket: bracketed!(content in input),
            effects: content.parse_terminated(Ident::parse, Token![,])?,
        })
    }
}

/// Composite key configuration: `composite_key: [field_name_one, field_name_two]`
pub struct CompositeKeyConfig {
    _bracket: token::Bracket,
    pub fields: Punctuated<Ident, Token![,]>,
}

impl Parse for CompositeKeyConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(CompositeKeyConfig {
            _bracket: bracketed!(content in input),
            fields: content.parse_terminated(Ident::parse, Token![,])?,
        })
    }
}

impl SchemaSpec {
    /// Lower parsed items into a [`ParsedSchema`].
    pub fn to_schema(&self) -> Result<ParsedSchema> {
        let mut table_name = None;
        let mut version = None;
        let mut description = None;
        let mut repository_top: Option<String> = None;
        let mut meta_repository: Option<String> = None;
        let mut retention = "365 days".to_string();
        let mut owner = "system".to_string();
        let mut ttl = None;
        let mut policies = None;
        let mut fields = Vec::new();
        let mut connections = Vec::new();
        let mut side_effects = Vec::new();
        let mut iters = Vec::new();
        let mut composite_key = Vec::new();
        let mut traits = Vec::new();
        let mut database = None;
        let mut ownership: Option<ParsedOwnershipConfig> = None;

        for item in &self.items {
            match item {
                SchemaItem::Table(t) => table_name = Some(t.value.value()),
                SchemaItem::Version(v) => version = Some(v.value.value()),
                SchemaItem::Description(d) => description = Some(d.value.value()),
                SchemaItem::Repository(r) => {
                    if repository_top.is_some() {
                        return Err(syn::Error::new(
                            self.name.span(),
                            "duplicate `repository:` in valence_schema!",
                        ));
                    }
                    repository_top = Some(r.value.value());
                }
                SchemaItem::Meta(m) => {
                    for mi in &m.items {
                        match mi {
                            MetaItem::Repository(v) => {
                                if meta_repository.is_some() {
                                    return Err(syn::Error::new(
                                        self.name.span(),
                                        "duplicate `repository` in meta: block",
                                    ));
                                }
                                meta_repository = Some(v.value());
                            }
                            MetaItem::Retention(v) => retention = v.value(),
                            MetaItem::Owner(v) => owner = v.value(),
                            MetaItem::Description(v) => {
                                if description.is_none() {
                                    description = Some(v.value());
                                }
                            }
                        }
                    }
                }
                SchemaItem::Ttl(t) => ttl = Some(parse_ttl_config(t)?),
                SchemaItem::Policies(p) => policies = Some(parse_policies_config(p)?),
                SchemaItem::Fields(f) => fields = parse_fields(f)?,
                SchemaItem::SideEffects(se) => {
                    side_effects = se.effects.iter().map(|i| i.to_string()).collect();
                }
                SchemaItem::Iters(it) => {
                    iters = it.effects.iter().map(|i| i.to_string()).collect();
                }
                SchemaItem::CompositeKey(ck) => {
                    composite_key = ck.fields.iter().map(|i| i.to_string()).collect();
                }
                SchemaItem::Traits(t) => {
                    traits = t.traits.iter().map(|i| i.to_string()).collect();
                }
                SchemaItem::Privacy(_) => {}
                SchemaItem::Connections(c) => connections = parse_connections(c)?,
                SchemaItem::Database(d) => {
                    if database.is_some() {
                        return Err(syn::Error::new(
                            self.name.span(),
                            "duplicate `database:` in valence_schema!",
                        ));
                    }
                    database = Some(d.expr.clone());
                }
                SchemaItem::Ownership(block) => {
                    if ownership.is_some() {
                        return Err(syn::Error::new(
                            self.name.span(),
                            "duplicate `ownership:` in valence_schema!",
                        ));
                    }
                    ownership = Some(parse_ownership_block(block, self.name.span())?);
                }
            }
        }

        let table_name =
            table_name.ok_or_else(|| syn::Error::new(self.name.span(), "Missing 'table' field"))?;
        let version = version.unwrap_or_else(|| "1.0.0".to_string());
        let repository = meta_repository
            .or(repository_top)
            .ok_or_else(|| {
                syn::Error::new(
                    self.name.span(),
                    "Missing required `repository` (top-level `repository: \"…\"` or `meta: { repository: \"…\" }`)",
                )
            })?;
        if repository.trim().is_empty() {
            return Err(syn::Error::new(
                self.name.span(),
                "`repository` must be a non-empty Git URL",
            ));
        }

        Ok(ParsedSchema {
            table_name,
            version,
            description,
            repository,
            retention,
            owner,
            database,
            ttl,
            policies,
            fields,
            connections,
            side_effects,
            iters,
            composite_key,
            traits,
            ownership,
        })
    }
}
