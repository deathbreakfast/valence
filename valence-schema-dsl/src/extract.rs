//! Normalize `FieldType::…` and `Validator::…` token trees into the compact strings
//! [`crate::ParsedField`] and validations use (e.g. `record<user>`, `min_length:10`).
//!
//! These helpers intentionally use [`TokenStream::to_string()`] and light string parsing
//! because the DSL accepts arbitrary `syn::Expr` fragments inside the macro; a fully
//! symbolic parser would duplicate `syn` AST matching for little gain here.

use proc_macro2::{Span, TokenStream};
use syn::{Error, Result};

/// Result of lowering a `r#type: FieldType::…` expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFieldType {
    /// Internal wire type (`json_as:…`, `record<table>`, `datetime`, …).
    pub field_type: String,
    /// Optional cross-crate / explicit model path from `Record(…).target(…)`.
    pub model_path: Option<String>,
}

/// Strip surrounding quotes for string literal defaults; otherwise return the raw token text.
pub fn extract_default_string(tokens: &TokenStream) -> String {
    let s = tokens.to_string();
    if s.starts_with('"') && s.ends_with('"') && s.len() > 1 {
        s[1..s.len() - 1].to_string()
    } else {
        s
    }
}

/// Lower a `r#type: FieldType::…` expression to Valence's internal type name.
///
/// Order matters: `JsonAs` / `ExternalEnum` / `Record(…).target(…)` must run before
/// naive `rfind("::")` splitting so paths with `::` inside the string stay intact.
pub fn extract_field_type_string(tokens: &TokenStream) -> Result<String> {
    Ok(extract_field_type(tokens)?.field_type)
}

/// Lower a field type expression, including optional Record `.target(…)` model path.
pub fn extract_field_type(tokens: &TokenStream) -> Result<ExtractedFieldType> {
    let s = tokens.to_string();

    if let Some(extracted) = parse_json_as(&s)? {
        return Ok(extracted);
    }
    if let Some(path) = parse_external_enum(&s) {
        if path.is_empty() {
            return Err(Error::new(
                Span::call_site(),
                "FieldType::ExternalEnum path must be non-empty",
            ));
        }
        return Ok(ExtractedFieldType {
            field_type: format!("ext_enum:{path}"),
            model_path: None,
        });
    }
    if let Some(variants) = parse_braced_enum_variants(&s) {
        return Ok(ExtractedFieldType {
            field_type: format!("enum:{}", variants.join(",")),
            model_path: None,
        });
    }
    if let Some(extracted) = parse_record(&s)? {
        return Ok(extracted);
    }

    let Some(idx) = s.rfind("::") else {
        return Ok(ExtractedFieldType {
            field_type: s.to_lowercase(),
            model_path: None,
        });
    };

    let ftype = s[idx + 2..].trim();
    // Reject `.target` / `.serde_error` chained onto non-Record / non-JsonAs.
    if ftype.contains(".target") || ftype.contains(". target") {
        return Err(Error::new(
            Span::call_site(),
            "FieldType::target(...) is only valid on FieldType::Record(...)",
        ));
    }
    if ftype.contains("serde_error") {
        return Err(Error::new(
            Span::call_site(),
            "FieldType::serde_error(...) is only valid on FieldType::JsonAs(...)",
        ));
    }

    Ok(ExtractedFieldType {
        field_type: ftype.to_lowercase(),
        model_path: None,
    })
}

fn parse_json_as(s: &str) -> Result<Option<ExtractedFieldType>> {
    let json_pos = match s.find("JsonAs") {
        Some(p) => p,
        None => return Ok(None),
    };
    // Avoid matching inside other identifiers.
    if json_pos > 0 {
        let before = s.as_bytes()[json_pos - 1];
        if before.is_ascii_alphanumeric() || before == b'_' {
            return Ok(None);
        }
    }

    let after = &s[json_pos..];
    let paren_start = after
        .find('(')
        .ok_or_else(|| Error::new(Span::call_site(), "FieldType::JsonAs requires (\"path\")"))?;
    let paren_end = after[paren_start..]
        .find(')')
        .map(|i| paren_start + i)
        .ok_or_else(|| Error::new(Span::call_site(), "FieldType::JsonAs missing closing ')'"))?;
    let param = after[paren_start + 1..paren_end].trim();
    let type_path = param.trim_matches('"').trim().replace(' ', "");
    if type_path.is_empty() {
        return Err(Error::new(
            Span::call_site(),
            "FieldType::JsonAs path must be non-empty",
        ));
    }

    let rest = after[paren_end + 1..].replace(' ', "");
    let mut serde_error = "error";
    if let Some(idx) = rest.find(".serde_error(") {
        let mode_start = idx + ".serde_error(".len();
        let mode_end = rest[mode_start..]
            .find(')')
            .map(|i| mode_start + i)
            .ok_or_else(|| {
                Error::new(
                    Span::call_site(),
                    "FieldType::JsonAs.serde_error(...) missing closing ')'",
                )
            })?;
        let mode = &rest[mode_start..mode_end];
        serde_error = if mode.contains("Panic") {
            "panic"
        } else if mode.contains("Error") {
            "error"
        } else {
            return Err(Error::new(
                Span::call_site(),
                "FieldType::JsonAs.serde_error expects JsonAsSerdeError::Panic or ::Error",
            ));
        };
    } else if rest.contains(".target(") {
        return Err(Error::new(
            Span::call_site(),
            "FieldType::target(...) is only valid on FieldType::Record(...)",
        ));
    }

    let field_type = if serde_error == "error" {
        format!("json_as:{type_path}")
    } else {
        format!("json_as:{type_path};serde_error={serde_error}")
    };

    Ok(Some(ExtractedFieldType {
        field_type,
        model_path: None,
    }))
}

fn parse_external_enum(s: &str) -> Option<String> {
    let ext_pos = s.find("ExternalEnum")?;
    let after = &s[ext_pos..];
    let paren_start = after.find('(')?;
    let paren_end = after.rfind(')')?;
    let param = after[paren_start + 1..paren_end].trim();
    Some(param.trim_matches('"').trim().replace(' ', ""))
}

/// `FieldType::Enum(&["A", "B"])` or `FieldType :: Enum (& ["A", "B"])` after `to_string()`.
fn parse_braced_enum_variants(s: &str) -> Option<Vec<String>> {
    // TokenStream spacing varies (`::Enum` vs `:: Enum`); accept both.
    let enum_pos = s.find(":: Enum").or_else(|| s.find("::Enum"))?;
    let after = s[enum_pos + 2..].trim_start();
    if !after.starts_with("Enum") {
        return None;
    }
    let paren_start = after.find('(')?;
    let paren_end = after.rfind(')')?;
    let param = after[paren_start + 1..paren_end].trim();
    let array_content = param
        .trim_start_matches('&')
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    let values: Vec<String> = array_content
        .split(',')
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
        .collect();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn parse_record(s: &str) -> Result<Option<ExtractedFieldType>> {
    // Find `Record (` after FieldType::
    let record_pos = match find_record_token(s) {
        Some(p) => p,
        None => return Ok(None),
    };
    let after = &s[record_pos..];
    let paren_start = after
        .find('(')
        .ok_or_else(|| Error::new(Span::call_site(), "FieldType::Record requires (\"table\")"))?;
    let paren_end = after[paren_start..]
        .find(')')
        .map(|i| paren_start + i)
        .ok_or_else(|| Error::new(Span::call_site(), "FieldType::Record missing closing ')'"))?;
    let param = after[paren_start + 1..paren_end].trim();
    let table = param.trim_matches('"').trim().to_string();
    if table.is_empty() {
        return Err(Error::new(
            Span::call_site(),
            "FieldType::Record table name must be non-empty",
        ));
    }

    let rest = after[paren_end + 1..].replace(' ', "");
    let mut model_path = None;
    if let Some(idx) = rest.find(".target(") {
        let path_start = idx + ".target(".len();
        let path_end = rest[path_start..]
            .find(')')
            .map(|i| path_start + i)
            .ok_or_else(|| {
                Error::new(
                    Span::call_site(),
                    "FieldType::Record.target(...) missing closing ')'",
                )
            })?;
        let path = rest[path_start..path_end]
            .trim_matches('"')
            .trim()
            .replace(' ', "");
        if path.is_empty() {
            return Err(Error::new(
                Span::call_site(),
                "FieldType::Record.target path must be non-empty",
            ));
        }
        model_path = Some(path);
    } else if rest.contains(".serde_error(") {
        return Err(Error::new(
            Span::call_site(),
            "FieldType::serde_error(...) is only valid on FieldType::JsonAs(...)",
        ));
    }

    Ok(Some(ExtractedFieldType {
        field_type: format!("record<{table}>"),
        model_path,
    }))
}

fn find_record_token(s: &str) -> Option<usize> {
    // Prefer `:: Record` / `::Record` to avoid false positives.
    if let Some(idx) = s.find(":: Record") {
        return Some(idx + ":: ".len());
    }
    if let Some(idx) = s.find("::Record") {
        return Some(idx + 2);
    }
    // TokenStream may produce `Record (` without space quirks after rfind path strip.
    s.find("Record(").or_else(|| s.find("Record ("))
}

/// Lower `Validator::…` tokens to validation DSL strings (`email`, `min_length:5`, …).
pub fn extract_validator_string(tokens: &TokenStream) -> Result<String> {
    let s = tokens.to_string();

    if let Some(idx) = s.find("::") {
        let rest = s[idx + 2..].trim();
        if let Some(k) = simple_validator_keyword(rest) {
            return Ok(k);
        }
        if let Some(v) = parameterized_validator(rest) {
            return Ok(v);
        }
    }

    Ok(if s.starts_with('"') && s.ends_with('"') && s.len() > 1 {
        s[1..s.len() - 1].to_string()
    } else {
        s
    })
}

fn simple_validator_keyword(rest: &str) -> Option<String> {
    const MAP: &[(&str, &str)] = &[
        ("Email", "email"),
        ("Phone", "phone"),
        ("Url", "url"),
        ("NonEmpty", "non_empty"),
        ("Positive", "positive"),
        ("NonNegative", "non_negative"),
    ];
    for (prefix, out) in MAP {
        if rest.starts_with(prefix) && !rest[prefix.len()..].starts_with('(') {
            return Some((*out).to_string());
        }
    }
    None
}

fn parameterized_validator(rest: &str) -> Option<String> {
    let paren_idx = rest.find('(')?;
    let end_paren = rest.rfind(')')?;
    let validator_name = rest[..paren_idx].trim();
    let param = rest[paren_idx + 1..end_paren].trim();

    match validator_name {
        "MinLength" => Some(format!("min_length:{param}")),
        "MaxLength" => Some(format!("max_length:{param}")),
        "Min" => Some(format!("min:{param}")),
        "Max" => Some(format!("max:{param}")),
        "Enum" => Some(format!("enum:{}", split_comma_list(param).join(","))),
        "Pattern" => Some(format!("pattern:{}", param.trim_matches('"'))),
        "Custom" => Some(format!("fn:{}", param.trim_matches('"'))),
        _ => None,
    }
}

fn split_comma_list(param: &str) -> Vec<String> {
    let array_content = param
        .trim_start_matches('&')
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    array_content
        .split(',')
        .map(|v| v.trim().trim_matches('"').to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn extract_json_as_default_error() {
        let ts = quote! { FieldType::JsonAs("crate::Payload") };
        let got = extract_field_type(&ts).unwrap();
        assert_eq!(got.field_type, "json_as:crate::Payload");
        assert!(got.model_path.is_none());
    }

    #[test]
    fn extract_json_as_panic() {
        let ts =
            quote! { FieldType::JsonAs("crate::Payload").serde_error(JsonAsSerdeError::Panic) };
        let got = extract_field_type(&ts).unwrap();
        assert_eq!(got.field_type, "json_as:crate::Payload;serde_error=panic");
    }

    #[test]
    fn extract_json_as_rejects_empty() {
        let ts = quote! { FieldType::JsonAs("") };
        assert!(extract_field_type(&ts).is_err());
    }

    #[test]
    fn extract_record_with_target() {
        let ts = quote! { FieldType::Record("user").target("other_crate::generated::User") };
        let got = extract_field_type(&ts).unwrap();
        assert_eq!(got.field_type, "record<user>");
        assert_eq!(
            got.model_path.as_deref(),
            Some("other_crate::generated::User")
        );
    }

    #[test]
    fn extract_record_rejects_empty_target() {
        let ts = quote! { FieldType::Record("user").target("") };
        assert!(extract_field_type(&ts).is_err());
    }

    #[test]
    fn extract_target_on_string_rejected() {
        let ts = quote! { FieldType::String.target("x") };
        assert!(extract_field_type(&ts).is_err());
    }

    #[test]
    fn extract_plain_json() {
        let ts = quote! { FieldType::Json };
        let got = extract_field_type(&ts).unwrap();
        assert_eq!(got.field_type, "json");
    }

    #[test]
    fn extract_currency() {
        let ts = quote! { FieldType::Currency };
        let got = extract_field_type(&ts).unwrap();
        assert_eq!(got.field_type, "currency");
    }
}
