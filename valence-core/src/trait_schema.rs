//! Trait schema metadata for `valence_trait_schema!`.

use crate::schema_api::SchemaPolicyRule;

/// Minimal field descriptor stored in trait definitions.
#[derive(Debug, Clone)]
pub struct TraitFieldDef {
    pub name: &'static str,
    pub field_type: &'static str,
    pub required: bool,
}

/// Policy rule names for a single CRUD operation bucket.
#[derive(Debug, Clone)]
pub struct TraitPolicyRules {
    pub always_allow: &'static [SchemaPolicyRule],
    pub allow: &'static [SchemaPolicyRule],
    pub block: &'static [SchemaPolicyRule],
    pub always_block: &'static [SchemaPolicyRule],
    /// Optional parent-edge defer (see schema `defer_to_edge` on any op).
    pub defer_to_edge: Option<&'static str>,
}

/// Entity-level privacy policies declared by a trait.
#[derive(Debug, Clone)]
pub struct TraitPolicies {
    pub read: Option<&'static TraitPolicyRules>,
    pub create: Option<&'static TraitPolicyRules>,
    pub update: Option<&'static TraitPolicyRules>,
    pub delete: Option<&'static TraitPolicyRules>,
}

/// Runtime metadata for a single Valence trait definition.
#[derive(Debug, Clone)]
pub struct TraitDefinition {
    pub name: &'static str,
    pub fields: &'static [TraitFieldDef],
    pub connection_names: &'static [&'static str],
    pub policies: Option<&'static TraitPolicies>,
}

/// Lazy initializer for trait definitions.
pub struct TraitDefinitionInit(pub fn() -> &'static TraitDefinition);

inventory::collect!(TraitDefinitionInit);

/// Links a concrete table to the trait it implements.
pub struct TraitImplementor {
    pub trait_name: &'static str,
    pub table_name: &'static str,
}

inventory::collect!(TraitImplementor);
