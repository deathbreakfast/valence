//! Normalized view of a parsed schema passed to all generators.
//!
//! Holds clones of frequently used slices (fields, composite key, traits) alongside the full
//! schema, plus emission-critical tokens (`database:`, policy exprs) that
//! [`valence_core::Schema`] cannot store at build time.

use syn::Expr;
use valence_core::{Schema, SchemaField};
use valence_schema_dsl::ParsedPolicies;

use crate::codegen::parser::ParsedSchemaFile;

/// Parsed schema plus denormalized copies for codegen (`table_name`, `fields`, `composite_key`, …).
#[allow(dead_code)]
pub struct SchemaContext {
    pub schema: Schema,
    pub table_name: String,
    pub fields: Vec<SchemaField>,
    pub side_effects: Vec<String>,
    pub iters: Vec<String>,
    pub composite_key: Vec<String>,
    pub traits: Vec<String>,
    /// Optional `database:` expression from the DSL (emitted into metadata).
    pub database: Option<Expr>,
    /// Policy rule expressions for metadata emission (leaked evaluators).
    pub policies: Option<ParsedPolicies>,
}

impl SchemaContext {
    /// Build context from a parsed schema file (after trait merge on `schema`).
    #[allow(clippy::unnecessary_wraps)] // Result kept for uniform generator API
    pub fn from_parsed(parsed: ParsedSchemaFile) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = parsed.schema;
        let table_name = schema.name.clone();
        let fields = schema.fields.clone();
        let side_effects = schema.side_effects.clone();
        let iters = schema.iters.clone();
        let composite_key = schema.composite_key.clone();
        let traits = schema.traits.clone();

        Ok(Self {
            schema,
            table_name,
            fields,
            side_effects,
            iters,
            composite_key,
            traits,
            database: parsed.database,
            policies: parsed.policies,
        })
    }
}
