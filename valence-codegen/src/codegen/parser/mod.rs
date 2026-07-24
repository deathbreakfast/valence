//! Schema and trait DSL parsing via shared [`valence_schema_dsl`].
//!
//! Host `schemas/*.rs` files are parsed as Rust sources; the single
//! `valence_schema!` / `valence_trait_schema!` body is lowered into generator IR.

mod lower;

use valence_core::SchemaConnection;
use valence_schema_dsl::{parse_schema_file, parse_trait_file, ParsedPolicies, ParsedSchema};

pub use lower::{lower_parsed_schema, lower_parsed_trait};

/// Parsed trait definition from a `valence_trait_schema!` file (generator IR).
#[derive(Debug, Clone)]
pub struct ParsedTraitDef {
    pub name: String,
    pub fields: Vec<valence_core::SchemaField>,
    pub connections: Vec<SchemaConnection>,
}

/// Result of parsing a schema file: core [`Schema`] plus emission-critical AST.
#[derive(Debug, Clone)]
pub struct ParsedSchemaFile {
    pub schema: valence_core::Schema,
    pub database: Option<syn::Expr>,
    pub policies: Option<ParsedPolicies>,
    /// Per-field policy AST (parallel to [`Schema::fields`]) for metadata emission.
    pub field_policies: Vec<Option<ParsedPolicies>>,
}

/// Parse a `valence_schema!` source file into generator IR.
pub fn extract_schema_from_macro(
    content: &str,
) -> Result<ParsedSchemaFile, Box<dyn std::error::Error>> {
    let parsed: ParsedSchema = parse_schema_file(content).map_err(|e| e.message)?;
    if let Some(expr) = &parsed.database {
        if matches!(
            expr,
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(_),
                ..
            })
        ) {
            return Err(
                "`database:` cannot be a string literal (no stable address for `&dyn DatabaseEvaluator`). \
Use a named `const` / `static` evaluator instead."
                    .into(),
            );
        }
    }
    let field_policies = parsed
        .fields
        .iter()
        .map(|f| f.policies.clone())
        .collect();
    Ok(ParsedSchemaFile {
        schema: lower_parsed_schema(&parsed),
        database: parsed.database,
        policies: parsed.policies,
        field_policies,
    })
}

/// Extract a trait definition from a `valence_trait_schema!` file.
pub fn extract_trait_from_file(
    content: &str,
) -> Result<ParsedTraitDef, Box<dyn std::error::Error>> {
    let parsed = parse_trait_file(content).map_err(|e| e.message)?;
    Ok(lower_parsed_trait(&parsed))
}

#[cfg(test)]
mod parser_tests;
