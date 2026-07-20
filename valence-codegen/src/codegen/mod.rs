//! Turn one schema source file into formatted Rust: merge traits, validate, run all generators.
//!
//! Used by [`crate::generate_models`] and by unit tests. Submodules: `parser`, `schema`,
//! `validation`, `generators`.

use proc_macro2::TokenStream;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

mod generators;
pub mod parser;
mod schema;
mod utils;
mod validation;

#[cfg(test)]
mod tests;

/// Read a `valence_schema!` file, merge any listed traits, validate, and return pretty-printed Rust source.
pub fn generate_from_schema_file(
    path: &Path,
    trait_defs: &HashMap<String, parser::ParsedTraitDef>,
) -> Result<String, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;

    let mut parsed_file = parser::extract_schema_from_macro(&content)?;

    // Merge trait fields and connections into the schema
    for trait_name in &parsed_file.schema.traits.clone() {
        if let Some(trait_def) = trait_defs.get(trait_name) {
            for field in &trait_def.fields {
                if !parsed_file
                    .schema
                    .fields
                    .iter()
                    .any(|f| f.name == field.name)
                {
                    let mut merged = field.clone();
                    if merged.field_type.starts_with("enum:") && merged.enum_type.is_none() {
                        merged.enum_type = Some(format!(
                            "{}{}",
                            trait_name,
                            utils::to_pascal_case(&merged.name)
                        ));
                    }
                    parsed_file.schema.fields.push(merged);
                }
            }
            for conn in &trait_def.connections {
                if !parsed_file
                    .schema
                    .connections
                    .iter()
                    .any(|c| c.name == conn.name)
                {
                    let mut merged = conn.clone();
                    merged.from_table.clone_from(&parsed_file.schema.name);
                    parsed_file.schema.connections.push(merged);
                }
            }
        }
    }

    validation::validate_connections_and_fields(&parsed_file.schema)
        .map_err(|e| format!("Schema validation failed: {e}"))?;

    let schema = schema::SchemaContext::from_parsed(parsed_file)?;

    let mut tokens = TokenStream::new();

    tokens.extend(generators::generate_struct(&schema)?);
    tokens.extend(generators::generate_connections(&schema)?);
    tokens.extend(generators::generate_side_effects(&schema)?);
    tokens.extend(generators::generate_iters(&schema)?);
    tokens.extend(generators::generate_crud_operations(&schema)?);
    tokens.extend(generators::generate_query_builder(&schema)?);
    tokens.extend(generators::generate_schema_metadata_method(&schema)?);

    // Generate trait impl blocks for each trait this schema implements
    tokens.extend(generators::generate_trait_impls(&schema, trait_defs)?);

    Ok(prettyplease::unparse(&syn::parse_file(
        &tokens.to_string(),
    )?))
}

/// Read a `valence_trait_schema!` file and return pretty-printed trait/query definition Rust source.
pub fn generate_from_trait_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let trait_def = parser::extract_trait_from_file(&content)?;

    let tokens = generators::generate_trait_definition(&trait_def)?;

    Ok(prettyplease::unparse(&syn::parse_file(
        &tokens.to_string(),
    )?))
}
