//! Schema- and field-level [`valence::SchemaPolicies`] token builders.
//!
//! Shared between schema registration and trait definitions that embed the
//! same policy rule shapes (`allow`, `block`, …).

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use valence_schema_dsl;

pub fn has_any_policy(policies: &valence_schema_dsl::ParsedPolicies) -> bool {
    fn rules_active(r: &valence_schema_dsl::ParsedPolicyRules) -> bool {
        !r.always_allow.is_empty()
            || !r.allow.is_empty()
            || !r.block.is_empty()
            || !r.always_block.is_empty()
            || r.defer_to_edge.is_some()
    }
    policies.read.as_ref().is_some_and(rules_active)
        || policies.create.as_ref().is_some_and(rules_active)
        || policies.update.as_ref().is_some_and(rules_active)
        || policies.delete.as_ref().is_some_and(rules_active)
}

/// Emits `Some(valence::SchemaPolicies { ... })` or `None` when no rules exist.
pub fn policies_tokens(policies: Option<&valence_schema_dsl::ParsedPolicies>) -> TokenStream2 {
    let Some(policies) = policies else {
        return quote! { None };
    };

    if !has_any_policy(policies) {
        return quote! { None };
    }

    let read = policy_rules_tokens(policies.read.as_ref());
    let create = policy_rules_tokens(policies.create.as_ref());
    let update = policy_rules_tokens(policies.update.as_ref());
    let delete = policy_rules_tokens(policies.delete.as_ref());

    quote! {
        Some(valence::SchemaPolicies {
            read: #read,
            create: #create,
            update: #update,
            delete: #delete,
        })
    }
}

pub fn policy_rules_tokens(rules: Option<&valence_schema_dsl::ParsedPolicyRules>) -> TokenStream2 {
    let Some(rules) = rules else {
        return quote! { None };
    };

    let always_allow = policy_rule_vec_tokens(&rules.always_allow);
    let allow = policy_rule_vec_tokens(&rules.allow);
    let block = policy_rule_vec_tokens(&rules.block);
    let always_block = policy_rule_vec_tokens(&rules.always_block);
    let defer_to_edge = if let Some(edge) = &rules.defer_to_edge {
        quote! { Some(#edge.to_string()) }
    } else {
        quote! { None }
    };

    quote! {
        Some(valence::SchemaPolicyRules {
            always_allow: #always_allow,
            allow: #allow,
            block: #block,
            always_block: #always_block,
            defer_to_edge: #defer_to_edge,
        })
    }
}

/// Wrap each policy expression as a leaked [`valence::PolicyEvaluator`] for static storage.
pub fn policy_rule_vec_tokens(values: &[TokenStream2]) -> TokenStream2 {
    if values.is_empty() {
        return quote! { Vec::new() };
    }

    let items: Vec<TokenStream2> = values
        .iter()
        .map(|rule| {
            quote! {
                {
                    let rule = #rule;
                    let evaluator: &'static dyn valence::PolicyEvaluator =
                        Box::leak(Box::new(rule.clone()));
                    valence::SchemaPolicyRule {
                        name: evaluator.name().to_string(),
                        description: evaluator.description().map(|desc| desc.to_string()),
                        evaluator: Some(evaluator),
                    }
                }
            }
        })
        .collect();

    quote! { vec![#(#items),*] }
}
