//! Extract `valence_schema!` / `valence_trait_schema!` from host schema `.rs` files.

use proc_macro2::TokenStream;
use syn::{Attribute, Item, Meta};

use crate::parse::{ParsedSchema, SchemaSpec};
use crate::trait_schema::{ParsedTraitSchema, TraitSchemaSpec};

/// Error locating or parsing a schema/trait macro in a source file.
#[derive(Debug)]
pub struct FileParseError {
    pub message: String,
}

impl std::fmt::Display for FileParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FileParseError {}

impl From<syn::Error> for FileParseError {
    fn from(value: syn::Error) -> Self {
        Self {
            message: value.to_string(),
        }
    }
}

/// Parse a schema file containing exactly one `valence_schema!` invocation.
///
/// # Errors
///
/// Returns an error when the file is not valid Rust, contains a legacy TOML raw-string
/// schema, or does not contain exactly one `valence_schema!` macro.
///
/// # Examples
///
/// ```
/// use valence_schema_dsl::parse_schema_file;
///
/// let src = r#"
///     use valence::prelude::*;
///     valence_schema! {
///         Widget {
///             repository: "https://github.com/unified-field-dev/valence",
///             table: "widget",
///             version: "0.1.0",
///             fields: [
///                 id: { r#type: FieldType::String, primary_key: true, required: true },
///             ],
///         }
///     }
/// "#;
/// let parsed = parse_schema_file(src).expect("parse");
/// assert_eq!(parsed.table_name, "widget");
/// ```
pub fn parse_schema_file(source: &str) -> Result<ParsedSchema, FileParseError> {
    reject_legacy_toml(source)?;
    let file = syn::parse_file(source)?;
    let tokens = find_unique_macro(&file, &["valence_schema"])?;
    let spec: SchemaSpec = syn::parse2(tokens)?;
    Ok(spec.to_schema()?)
}

/// Parse a trait file containing exactly one `valence_trait_schema!` invocation.
///
/// # Errors
///
/// Returns an error when the file is not valid Rust or does not contain exactly one
/// `valence_trait_schema!` macro.
pub fn parse_trait_file(source: &str) -> Result<ParsedTraitSchema, FileParseError> {
    reject_legacy_toml(source)?;
    let file = syn::parse_file(source)?;
    let tokens = find_unique_macro(&file, &["valence_trait_schema"])?;
    let spec: TraitSchemaSpec = syn::parse2(tokens)?;
    Ok(spec.to_parsed()?)
}

fn reject_legacy_toml(source: &str) -> Result<(), FileParseError> {
    if source.contains("r#\"") {
        return Err(FileParseError {
            message: "TOML schema literals are no longer supported; use DSL syntax".into(),
        });
    }
    Ok(())
}

fn find_unique_macro(file: &syn::File, names: &[&str]) -> Result<TokenStream, FileParseError> {
    let mut found: Vec<TokenStream> = Vec::new();
    collect_macros_from_attrs(&file.attrs, names, &mut found);
    for item in &file.items {
        collect_macros_from_item(item, names, &mut found);
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(FileParseError {
            message: format!(
                "expected exactly one {}! in file, found none",
                names.join(" / ")
            ),
        }),
        n => Err(FileParseError {
            message: format!(
                "expected exactly one {}! in file, found {n}",
                names.join(" / ")
            ),
        }),
    }
}

fn collect_macros_from_item(item: &Item, names: &[&str], out: &mut Vec<TokenStream>) {
    match item {
        Item::Macro(m) => {
            if let Some(ts) = macro_tokens_if_named(&m.mac, names) {
                out.push(ts);
            }
        }
        Item::Mod(m) => {
            collect_macros_from_attrs(&m.attrs, names, out);
            if let Some((_, items)) = &m.content {
                for nested in items {
                    collect_macros_from_item(nested, names, out);
                }
            }
        }
        Item::Use(u) => collect_macros_from_attrs(&u.attrs, names, out),
        Item::Const(c) => collect_macros_from_attrs(&c.attrs, names, out),
        Item::Static(s) => collect_macros_from_attrs(&s.attrs, names, out),
        Item::Fn(f) => {
            collect_macros_from_attrs(&f.attrs, names, out);
            // Macro invocations as statements are uncommon in schema files; skip.
        }
        _ => {}
    }
}

fn collect_macros_from_attrs(attrs: &[Attribute], names: &[&str], out: &mut Vec<TokenStream>) {
    for attr in attrs {
        if let Meta::List(list) = &attr.meta {
            let path = list.path.to_token_string();
            if names
                .iter()
                .any(|n| path == *n || path.ends_with(&format!("::{n}")))
            {
                out.push(list.tokens.clone());
            }
        }
    }
}

fn macro_tokens_if_named(mac: &syn::Macro, names: &[&str]) -> Option<TokenStream> {
    let path = mac.path.to_token_string();
    if names
        .iter()
        .any(|n| path == *n || path.ends_with(&format!("::{n}")))
    {
        Some(mac.tokens.clone())
    } else {
        None
    }
}

trait PathString {
    fn to_token_string(&self) -> String;
}

impl PathString for syn::Path {
    fn to_token_string(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_schema_file_with_use() {
        let src = r#"
            use valence::prelude::*;
            valence_schema! {
                Widget {
                repository: "https://github.com/unified-field-dev/valence",
                    table: "widget",
                    version: "0.1.0",
                    fields: [
                        id: { r#type: FieldType::String, primary_key: true, required: true },
                    ],
                }
            }
        "#;
        let parsed = parse_schema_file(src).expect("parse");
        assert_eq!(parsed.table_name, "widget");
        assert!(parsed.database.is_none());
    }

    #[test]
    fn parses_database_expr_from_file() {
        let src = r#"
            use crate::PROJECT_DB;
            valence_schema! {
                Project {
                repository: "https://github.com/unified-field-dev/valence",
                    table: "project",
                    version: "0.1.0",
                    database: PROJECT_DB,
                    fields: [
                        id: { r#type: FieldType::String, primary_key: true, required: true },
                    ],
                }
            }
        "#;
        let parsed = parse_schema_file(src).expect("parse");
        assert!(parsed.database.is_some());
    }

    #[test]
    fn rejects_zero_macros() {
        let err = parse_schema_file("fn main() {}").unwrap_err();
        assert!(err.message.contains("found none"));
    }

    #[test]
    fn rejects_two_macros() {
        let src = r#"
            valence_schema! { A { table: "a", fields: [] } }
            valence_schema! { B { table: "b", fields: [] } }
        "#;
        let err = parse_schema_file(src).unwrap_err();
        assert!(err.message.contains("found 2"));
    }

    #[test]
    fn rejects_legacy_toml() {
        let src = "valence_schema! { r#\"foo\"# }";
        let err = parse_schema_file(src).unwrap_err();
        assert!(err.message.contains("TOML schema literals"));
    }

    #[test]
    fn parses_trait_file() {
        let src = r#"
            valence_trait_schema! {
                Named {
                    repository: "https://github.com/unified-field-dev/valence",
                    fields: [
                        name: { r#type: FieldType::String, required: true },
                    ],
                }
            }
        "#;
        let parsed = parse_trait_file(src).expect("parse");
        assert_eq!(parsed.name, "Named");
        assert_eq!(parsed.fields.len(), 1);
    }
}
