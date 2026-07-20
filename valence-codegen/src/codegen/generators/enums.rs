//! Enum types for `enum:` DSL fields and naming helpers.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Convert SCREAMING_SNAKE_CASE to PascalCase.
/// e.g. "IN_PROGRESS" -> "InProgress", "PENDING" -> "Pending"
pub fn screaming_snake_to_pascal(s: &str) -> String {
    s.split('_')
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Generate a Rust enum definition from variant strings.
///
/// `enum_name`: PascalCase name for the enum (e.g. "TestStatusPhase")
/// `variants`: SCREAMING_SNAKE_CASE variant names (e.g. ["PENDING", "IN_PROGRESS"])
pub fn generate_enum_definition(enum_name: &str, variants: &[String]) -> TokenStream {
    let enum_ident = format_ident!("{}", enum_name);

    let variant_entries: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let pascal = screaming_snake_to_pascal(v);
            let variant_ident = format_ident!("{}", pascal);
            let rename_lit = v.as_str();
            quote! {
                #[serde(rename = #rename_lit)]
                #variant_ident
            }
        })
        .collect();

    let as_str_arms: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let pascal = screaming_snake_to_pascal(v);
            let variant_ident = format_ident!("{}", pascal);
            let lit = v.as_str();
            quote! { #enum_ident::#variant_ident => #lit }
        })
        .collect();

    let from_str_arms: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let pascal = screaming_snake_to_pascal(v);
            let variant_ident = format_ident!("{}", pascal);
            let lit = v.as_str();
            quote! { #lit => Some(#enum_ident::#variant_ident) }
        })
        .collect();

    quote! {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum #enum_ident {
            #(#variant_entries),*
        }

        #[allow(dead_code)]
        impl #enum_ident {
            pub fn as_str(&self) -> &'static str {
                match self {
                    #(#as_str_arms),*
                }
            }

            pub fn from_str(s: &str) -> Option<Self> {
                match s {
                    #(#from_str_arms,)*
                    _ => None,
                }
            }
        }

        impl std::fmt::Display for #enum_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn test_screaming_snake_to_pascal() {
        assert_eq!(screaming_snake_to_pascal("PENDING"), "Pending");
        assert_eq!(screaming_snake_to_pascal("IN_PROGRESS"), "InProgress");
        assert_eq!(screaming_snake_to_pascal("COMPLETED"), "Completed");
        assert_eq!(screaming_snake_to_pascal("NOT_FOUND"), "NotFound");
    }

    #[test]
    fn test_generate_enum_definition_compiles() {
        let tokens = generate_enum_definition(
            "TestPhase",
            &[
                "PENDING".to_string(),
                "IN_PROGRESS".to_string(),
                "COMPLETED".to_string(),
            ],
        );
        let code = tokens.to_string();
        assert!(code.contains("pub enum TestPhase"));
        assert!(code.contains("Pending"));
        assert!(code.contains("InProgress"));
        assert!(code.contains("Completed"));
        assert!(code.contains("as_str"));
        assert!(code.contains("from_str"));
        assert!(code.contains("Display"));
    }
}
