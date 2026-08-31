use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    token, Expr, Ident, LitBool, Result, Token,
};

/// Table- and field-level policy bundles lowered to token streams for codegen.
#[derive(Debug, Clone, Default)]
pub struct ParsedPolicies {
    pub read: Option<ParsedPolicyRules>,
    pub create: Option<ParsedPolicyRules>,
    pub update: Option<ParsedPolicyRules>,
    pub delete: Option<ParsedPolicyRules>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedPolicyRules {
    pub always_allow: Vec<TokenStream>,
    pub allow: Vec<TokenStream>,
    pub block: Vec<TokenStream>,
    pub always_block: Vec<TokenStream>,
    /// Named HasOne / Record edge whose parent Read gates this row (read policies only).
    pub defer_to_edge: Option<String>,
}

/// Parsed `privacy: { ... }` container (values are not carried into [`super::schema::ParsedSchema`]).
#[allow(dead_code)]
pub struct PrivacyConfig {
    _brace: token::Brace,
    pub items: Punctuated<PrivacyItem, Token![,]>,
}

impl Parse for PrivacyConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(PrivacyConfig {
            _brace: braced!(content in input),
            items: content.parse_terminated(PrivacyItem::parse, Token![,])?,
        })
    }
}

pub enum PrivacyItem {
    /// `gdpr_compliant:` literal accepted for source compatibility.
    GdprCompliant,
}

impl Parse for PrivacyItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        match key.to_string().as_str() {
            "gdpr_compliant" => {
                let _: LitBool = input.parse()?;
                Ok(PrivacyItem::GdprCompliant)
            }
            _ => Err(syn::Error::new(
                key.span(),
                format!("Unknown privacy key: {key}"),
            )),
        }
    }
}

pub struct PoliciesConfig {
    _brace: token::Brace,
    pub items: Punctuated<PolicyItem, Token![,]>,
}

impl Parse for PoliciesConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(PoliciesConfig {
            _brace: braced!(content in input),
            items: content.parse_terminated(PolicyItem::parse, Token![,])?,
        })
    }
}

pub enum PolicyItem {
    Read(PolicySpec),
    Create(PolicySpec),
    Update(PolicySpec),
    Delete(PolicySpec),
}

impl Parse for PolicyItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        match key.to_string().as_str() {
            "read" => Ok(PolicyItem::Read(input.parse()?)),
            "create" => Ok(PolicyItem::Create(input.parse()?)),
            "update" => Ok(PolicyItem::Update(input.parse()?)),
            "delete" => Ok(PolicyItem::Delete(input.parse()?)),
            _ => Err(syn::Error::new(
                key.span(),
                format!("Unknown policy key: {key}"),
            )),
        }
    }
}

/// A policy specification (PrivacyPolicy with rule arrays and optional defer_to_edge)
pub struct PolicySpec {
    _brace: token::Brace,
    pub items: Punctuated<PolicySpecEntry, Token![,]>,
}

impl Parse for PolicySpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(PolicySpec {
            _brace: braced!(content in input),
            items: content.parse_terminated(PolicySpecEntry::parse, Token![,])?,
        })
    }
}

/// One entry inside `read:` / `create:` / … — either a rule list or `defer_to_edge`.
pub enum PolicySpecEntry {
    Rules(PolicyRuleList),
    DeferToEdge(String),
}

impl Parse for PolicySpecEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        if name == "defer_to_edge" {
            let edge = if input.peek(syn::LitStr) {
                input.parse::<syn::LitStr>()?.value()
            } else {
                input.parse::<Ident>()?.to_string()
            };
            if edge.is_empty() {
                return Err(syn::Error::new(
                    name.span(),
                    "defer_to_edge requires a non-empty edge name",
                ));
            }
            return Ok(PolicySpecEntry::DeferToEdge(edge));
        }

        let content;
        let bracket = bracketed!(content in input);
        let mut rules = Punctuated::new();

        while !content.is_empty() {
            let expr: Expr = content.parse()?;
            rules.push_value(expr);

            if !content.is_empty() {
                rules.push_punct(content.parse()?);
            }
        }

        Ok(PolicySpecEntry::Rules(PolicyRuleList {
            name,
            _bracket: bracket,
            rules,
        }))
    }
}

/// A list of policy rules (e.g., `allow: [PUBLIC_READ, AUTHENTICATED]`)
pub struct PolicyRuleList {
    pub name: Ident,
    _bracket: token::Bracket,
    pub rules: Punctuated<Expr, Token![,]>,
}

pub fn parse_policies_config(config: &PoliciesConfig) -> Result<ParsedPolicies> {
    let mut parsed = ParsedPolicies::default();

    for item in &config.items {
        match item {
            PolicyItem::Read(spec) => parsed.read = Some(parse_policy_spec(spec)?),
            PolicyItem::Create(spec) => parsed.create = Some(parse_policy_spec(spec)?),
            PolicyItem::Update(spec) => parsed.update = Some(parse_policy_spec(spec)?),
            PolicyItem::Delete(spec) => parsed.delete = Some(parse_policy_spec(spec)?),
        }
    }

    Ok(parsed)
}

fn parse_policy_spec(spec: &PolicySpec) -> Result<ParsedPolicyRules> {
    let mut rules = ParsedPolicyRules::default();

    for entry in &spec.items {
        match entry {
            PolicySpecEntry::DeferToEdge(edge) => {
                if rules.defer_to_edge.is_some() {
                    return Err(syn::Error::new(
                        proc_macro2::Span::call_site(),
                        "duplicate defer_to_edge in policy block",
                    ));
                }
                rules.defer_to_edge = Some(edge.clone());
            }
            PolicySpecEntry::Rules(list) => {
                let rule_tokens: Vec<TokenStream> = list
                    .rules
                    .iter()
                    .map(|expr| expr.to_token_stream())
                    .collect();

                match list.name.to_string().as_str() {
                    "always_allow" => rules.always_allow.extend(rule_tokens),
                    "allow" => rules.allow.extend(rule_tokens),
                    "block" => rules.block.extend(rule_tokens),
                    "always_block" => rules.always_block.extend(rule_tokens),
                    other => {
                        return Err(syn::Error::new(
                            list.name.span(),
                            format!(
                                "Unknown policy rule key: {other} (expected always_allow, allow, block, always_block, or defer_to_edge)"
                            ),
                        ));
                    }
                }
            }
        }
    }

    Ok(rules)
}
