//! Emits `valence::TraitDefinition` statics and `TraitDefinitionInit` inventory hooks for
//! [`crate::valence_trait_schema`].

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::LitStr;

use crate::codegen::policies;
use valence_schema_dsl;

/// Expand `valence_trait_schema! { ... }`.
pub fn expand(input: TokenStream) -> TokenStream {
    let spec = match syn::parse::<valence_schema_dsl::TraitSchemaSpec>(input) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error().into(),
    };

    let parsed = match spec.to_parsed() {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    expand_parsed(&parsed).into()
}

fn expand_parsed(parsed: &valence_schema_dsl::ParsedTraitSchema) -> TokenStream2 {
    let trait_name_lit = LitStr::new(&parsed.name, proc_macro2::Span::call_site());

    let field_defs: Vec<TokenStream2> = parsed
        .fields
        .iter()
        .map(|f| {
            let name_lit = LitStr::new(&f.name, proc_macro2::Span::call_site());
            let type_lit = LitStr::new(&f.field_type, proc_macro2::Span::call_site());
            let required = f.required;
            quote! {
                valence::TraitFieldDef {
                    name: #name_lit,
                    field_type: #type_lit,
                    required: #required,
                }
            }
        })
        .collect();

    let conn_name_lits: Vec<LitStr> = parsed
        .connection_names()
        .iter()
        .map(|n| LitStr::new(n, proc_macro2::Span::call_site()))
        .collect();

    let policies_code = trait_policies_tokens(parsed.policies.as_ref());

    quote! {
        #[cfg(not(target_family = "wasm"))]
        const _: () = {
            fn __trait_definition() -> &'static valence::TraitDefinition {
                use std::sync::OnceLock;
                static DEF: OnceLock<valence::TraitDefinition> = OnceLock::new();
                DEF.get_or_init(|| {
                    static FIELDS: OnceLock<Vec<valence::TraitFieldDef>> = OnceLock::new();
                    let fields = FIELDS.get_or_init(|| vec![#(#field_defs),*]);
                    static CONN_NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
                    let conn_names = CONN_NAMES.get_or_init(|| vec![#(#conn_name_lits),*]);
                    #policies_code
                    valence::TraitDefinition {
                        name: #trait_name_lit,
                        fields: fields.as_slice(),
                        connection_names: conn_names.as_slice(),
                        policies: __trait_policies,
                    }
                })
            }

            valence::inventory::submit! {
                valence::TraitDefinitionInit(__trait_definition)
            }
        };
    }
}

/// Binds `__trait_policies` to `Option<&'static TraitPolicies>` using `OnceLock` statics.
fn trait_policies_tokens(policies: Option<&valence_schema_dsl::ParsedPolicies>) -> TokenStream2 {
    let Some(policies) = policies else {
        return quote! { let __trait_policies: Option<&'static valence::TraitPolicies> = None; };
    };

    if !policies::has_any_policy(policies) {
        return quote! { let __trait_policies: Option<&'static valence::TraitPolicies> = None; };
    }

    let read = trait_policy_rules_tokens("READ", policies.read.as_ref());
    let create = trait_policy_rules_tokens("CREATE", policies.create.as_ref());
    let update = trait_policy_rules_tokens("UPDATE", policies.update.as_ref());
    let delete = trait_policy_rules_tokens("DELETE", policies.delete.as_ref());

    quote! {
        #read
        #create
        #update
        #delete
        static TRAIT_POLICIES: OnceLock<valence::TraitPolicies> = OnceLock::new();
        let __trait_policies: Option<&'static valence::TraitPolicies> = Some(TRAIT_POLICIES.get_or_init(|| {
            valence::TraitPolicies {
                read: __trait_read_rules,
                create: __trait_create_rules,
                update: __trait_update_rules,
                delete: __trait_delete_rules,
            }
        }));
    }
}

fn trait_policy_rules_tokens(
    op_upper: &str,
    rules: Option<&valence_schema_dsl::ParsedPolicyRules>,
) -> TokenStream2 {
    let binding_name = syn::Ident::new(
        &format!("__trait_{}_rules", op_upper.to_lowercase()),
        proc_macro2::Span::call_site(),
    );

    let Some(rules) = rules else {
        return quote! {
            let #binding_name: Option<&'static valence::TraitPolicyRules> = None;
        };
    };

    let op_lower = op_upper.to_lowercase();

    let aa_static = syn::Ident::new(
        &format!("TRAIT_{op_upper}_ALWAYS_ALLOW"),
        proc_macro2::Span::call_site(),
    );
    let a_static = syn::Ident::new(
        &format!("TRAIT_{op_upper}_ALLOW"),
        proc_macro2::Span::call_site(),
    );
    let b_static = syn::Ident::new(
        &format!("TRAIT_{op_upper}_BLOCK"),
        proc_macro2::Span::call_site(),
    );
    let ab_static = syn::Ident::new(
        &format!("TRAIT_{op_upper}_ALWAYS_BLOCK"),
        proc_macro2::Span::call_site(),
    );
    let rules_static = syn::Ident::new(
        &format!("TRAIT_{op_upper}_RULES"),
        proc_macro2::Span::call_site(),
    );

    let aa_let = syn::Ident::new(
        &format!("__trait_{op_lower}_aa"),
        proc_macro2::Span::call_site(),
    );
    let a_let = syn::Ident::new(
        &format!("__trait_{op_lower}_a"),
        proc_macro2::Span::call_site(),
    );
    let b_let = syn::Ident::new(
        &format!("__trait_{op_lower}_b"),
        proc_macro2::Span::call_site(),
    );
    let ab_let = syn::Ident::new(
        &format!("__trait_{op_lower}_ab"),
        proc_macro2::Span::call_site(),
    );

    let aa_block = once_lock_leaked_rules_vec(&aa_static, &aa_let, &rules.always_allow);
    let a_block = once_lock_leaked_rules_vec(&a_static, &a_let, &rules.allow);
    let b_block = once_lock_leaked_rules_vec(&b_static, &b_let, &rules.block);
    let ab_block = once_lock_leaked_rules_vec(&ab_static, &ab_let, &rules.always_block);
    let defer_to_edge = if let Some(edge) = &rules.defer_to_edge {
        quote! { Some(#edge) }
    } else {
        quote! { None }
    };

    quote! {
        #aa_block
        #a_block
        #b_block
        #ab_block
        static #rules_static: OnceLock<valence::TraitPolicyRules> = OnceLock::new();
        let #binding_name: Option<&'static valence::TraitPolicyRules> = Some(#rules_static.get_or_init(|| {
            valence::TraitPolicyRules {
                always_allow: #aa_let.as_slice(),
                allow: #a_let.as_slice(),
                block: #b_let.as_slice(),
                always_block: #ab_let.as_slice(),
                defer_to_edge: #defer_to_edge,
            }
        }));
    }
}

fn once_lock_leaked_rules_vec(
    static_id: &syn::Ident,
    let_binding: &syn::Ident,
    rule_exprs: &[TokenStream2],
) -> TokenStream2 {
    quote! {
        static #static_id: OnceLock<Vec<valence::SchemaPolicyRule>> = OnceLock::new();
        let #let_binding = #static_id.get_or_init(|| {
            vec![
                #(
                    {
                        let rule = #rule_exprs;
                        let evaluator: &'static dyn valence::PolicyEvaluator =
                            Box::leak(Box::new(rule.clone()));
                        valence::SchemaPolicyRule {
                            name: evaluator.name().to_string(),
                            description: evaluator.description().map(|d| d.to_string()),
                            evaluator: Some(evaluator),
                        }
                    }
                ),*
            ]
        });
    }
}
