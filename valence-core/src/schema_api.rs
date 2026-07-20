//! Structured schema metadata produced by the Valence DSL.

use crate::evaluator::DatabaseEvaluator;
use crate::owner_ref::OwnershipConfig;
use crate::privacy::PolicyEvaluator;
use crate::ttl::SchemaTtlPolicy;
use serde::{Deserialize, Serialize};

/// One policy rule reference stored in schema metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaPolicyRule {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip, default)]
    pub evaluator: Option<&'static dyn PolicyEvaluator>,
}

/// Policy rule buckets for a single CRUD operation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaPolicyRules {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub always_allow: Vec<SchemaPolicyRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<SchemaPolicyRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub block: Vec<SchemaPolicyRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub always_block: Vec<SchemaPolicyRule>,
}

/// Full entity-level policy declaration for the schema.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaPolicies {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<SchemaPolicyRules>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<SchemaPolicyRules>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<SchemaPolicyRules>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<SchemaPolicyRules>,
}

/// Top-level privacy summary for a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaPrivacy {
    pub read: String,
    pub write: String,
}

/// Normalized foreign-key/reference metadata for a field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyRef {
    pub ref_table: String,
    pub field: String,
}

/// Expanded metadata for one field declared in `fields: [...]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    pub field_type: String,
    pub primary: bool,
    pub nullable: bool,
    pub indexed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fk: Option<ForeignKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policies: Option<SchemaPolicies>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub encrypted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_variants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_type: Option<String>,
}

/// Edge/relationship definition retained for older schema shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEdge {
    pub from_field: String,
    pub to_table: String,
    pub label: String,
}

/// Connection/relationship definition from schema DSL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConnection {
    pub name: String,
    pub from_table: String,
    pub from_field: String,
    pub to_table: String,
    pub cardinality: String,
    pub required: bool,
    pub on_delete: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_trait: Option<String>,
}

impl From<SchemaEdge> for SchemaConnection {
    fn from(edge: SchemaEdge) -> Self {
        let label = edge.label.clone();
        Self {
            name: edge.from_field.clone(),
            from_table: String::new(),
            from_field: edge.from_field,
            to_table: edge.to_table,
            cardinality: "HasOne".to_string(),
            required: true,
            on_delete: "Cascade".to_string(),
            label,
            model_path: None,
            reverse_field: None,
            edge_table: None,
            target_trait: None,
        }
    }
}

/// Schema metadata summary block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMeta {
    pub retention: String,
    pub row_count: u64,
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Complete runtime schema produced from `valence_schema!`.
#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    pub name: String,
    pub version: String,
    pub databases: Vec<String>,
    #[serde(skip)]
    pub database_evaluator: &'static dyn DatabaseEvaluator,
    pub privacy: SchemaPrivacy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policies: Option<SchemaPolicies>,
    pub fields: Vec<SchemaField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<SchemaEdge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<SchemaConnection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub side_effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub iters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub composite_key: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<SchemaTtlPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<OwnershipConfig>,
    pub meta: SchemaMeta,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's `skip_serializing_if` callback signature passes field values by reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}
