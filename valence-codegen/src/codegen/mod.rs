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
    drop_field_reads_that_never_hide(&mut parsed_file.schema);

    // Merge trait fields and connections into the schema
    for trait_name in &parsed_file.schema.traits.clone() {
        if let Some(trait_def) = trait_defs.get(trait_name) {
            for field in &trait_def.fields {
                // The trait's getter must hold for every implementor, so a schema can't
                // hide a field the trait promises is always there.
                if let Some(own) = parsed_file
                    .schema
                    .fields
                    .iter()
                    .find(|f| f.name == field.name)
                {
                    if own.read_hidable() && !field.stored_optional() {
                        return Err(format!(
                            "{}.{}: the field has a read policy, but trait {} declares it required \
                             with none, so its getter can't return a value for viewers the policy \
                             denies. Declare the read policy on the trait field.",
                            parsed_file.schema.name, field.name, trait_name
                        )
                        .into());
                    }
                }
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
                    // Trait-lowered fields currently omit evaluator AST; keep parallel None.
                    parsed_file.field_policies.push(None);
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

/// Clear, in codegen's copy of the schema only, field `read` policies that can't hide
/// the field from anyone who can read the row, so the generated model keeps the field
/// required. Runtime metadata keeps and enforces every policy; this only decides the
/// Rust shape.
///
/// A field policy is redundant when nothing blocks it, every rule that lets a viewer
/// read the row also lets them read the field, System passes it (System reads rows
/// without entity checks; owner rules always admit System), and the row read doesn't defer to an edge parent (that
/// admits viewers no allow rule names).
fn drop_field_reads_that_never_hide(schema: &mut valence_core::Schema) {
    fn short(rule: &valence_core::SchemaPolicyRule) -> &str {
        rule.name.rsplit("::").next().unwrap_or(&rule.name)
    }
    let entity_read = schema.policies.as_ref().and_then(|p| p.read.as_ref());
    if entity_read.is_some_and(|r| r.defer_to_edge.is_some()) {
        return;
    }
    let row_admits: Vec<&str> = entity_read
        .map(|r| r.always_allow.iter().chain(&r.allow).map(short).collect())
        .unwrap_or_default();
    for field in &mut schema.fields {
        let Some(read) = field.policies.as_ref().and_then(|p| p.read.as_ref()) else {
            continue;
        };
        let field_admits: Vec<&str> = read
            .always_allow
            .iter()
            .chain(&read.allow)
            .map(short)
            .collect();
        let system_passes = field_admits.iter().any(|n| {
            matches!(*n, "SYSTEM_ONLY" | "PUBLIC_READ" | "AUTHENTICATED")
                || n.starts_with("OWNER_BY_")
        });
        let redundant = read.block.is_empty()
            && read.always_block.is_empty()
            && system_passes
            && row_admits.iter().all(|n| field_admits.contains(n));
        if redundant {
            if let Some(p) = field.policies.as_mut() {
                p.read = None;
            }
        }
    }
}
