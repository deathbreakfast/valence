//! Lower [`valence_schema_dsl`] AST into [`valence_core::Schema`] generator IR.

use valence_core::{
    DatabaseEvaluator, ForeignKeyRef, OwnershipConfig, Schema, SchemaConnection, SchemaEdge,
    SchemaField, SchemaMeta, SchemaPolicies, SchemaPrivacy, SchemaTtlPolicy, DEFAULT_IN_MEMORY,
};
use valence_schema_dsl::{
    ParsedConnection, ParsedField, ParsedOwnershipConfig, ParsedPolicies, ParsedPolicyRules,
    ParsedSchema, ParsedTraitSchema, ParsedTtlPolicy,
};

use super::ParsedTraitDef;

/// Lower a parsed schema file AST into a [`Schema`] (placeholder DB evaluator for build-time IR).
pub fn lower_parsed_schema(parsed: &ParsedSchema) -> Schema {
    let (fields, edges) = lower_fields(&parsed.fields);
    let mut connections = lower_connections(&parsed.connections, &parsed.table_name);
    if connections.is_empty() {
        connections = fields
            .iter()
            .filter(|f| f.fk.is_some())
            .map(|f| {
                let ref_table =
                    f.fk.as_ref()
                        .map(|fk| fk.ref_table.clone())
                        .unwrap_or_default();
                SchemaConnection {
                    name: f.name.clone(),
                    from_table: parsed.table_name.clone(),
                    from_field: f.name.clone(),
                    to_table: ref_table,
                    cardinality: "HasOne".to_string(),
                    required: true,
                    on_delete: "Cascade".to_string(),
                    label: edge_label(&f.name),
                    model_path: f.model_path.clone(),
                    reverse_field: None,
                    edge_table: None,
                    target_trait: None,
                }
            })
            .collect();
    }

    Schema {
        name: parsed.table_name.clone(),
        version: parsed.version.clone(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "public".to_string(),
            write: "service".to_string(),
        },
        policies: parsed.policies.as_ref().map(lower_policies_names_only),
        fields,
        edges,
        connections,
        side_effects: parsed.side_effects.clone(),
        iters: parsed.iters.clone(),
        composite_key: parsed.composite_key.clone(),
        traits: parsed.traits.clone(),
        ttl: parsed.ttl.as_ref().map(lower_ttl),
        ownership: parsed.ownership.as_ref().map(lower_ownership),
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: parsed.description.clone(),
        },
    }
}

/// Lower a parsed trait file into codegen's [`ParsedTraitDef`].
pub fn lower_parsed_trait(parsed: &ParsedTraitSchema) -> ParsedTraitDef {
    let (fields, _edges) = lower_fields(&parsed.fields);
    let connections = lower_connections(&parsed.connections, "__trait__");
    ParsedTraitDef {
        name: parsed.name.clone(),
        fields,
        connections,
    }
}

fn lower_ttl(ttl: &ParsedTtlPolicy) -> SchemaTtlPolicy {
    SchemaTtlPolicy {
        seconds: ttl.seconds,
        mode: ttl.mode.clone(),
    }
}

fn lower_ownership(o: &ParsedOwnershipConfig) -> OwnershipConfig {
    OwnershipConfig {
        system_owned: o.system_owned,
        resolve: o.resolve.clone(),
    }
}

/// Name-only policy stubs for the build-time [`Schema`] value (evaluators are emitted from AST).
fn lower_policies_names_only(policies: &ParsedPolicies) -> SchemaPolicies {
    SchemaPolicies {
        read: policies.read.as_ref().map(rules_names_only),
        create: policies.create.as_ref().map(rules_names_only),
        update: policies.update.as_ref().map(rules_names_only),
        delete: policies.delete.as_ref().map(rules_names_only),
    }
}

fn rules_names_only(rules: &ParsedPolicyRules) -> valence_core::SchemaPolicyRules {
    valence_core::SchemaPolicyRules {
        always_allow: rule_names(&rules.always_allow),
        allow: rule_names(&rules.allow),
        block: rule_names(&rules.block),
        always_block: rule_names(&rules.always_block),
        defer_to_edge: rules.defer_to_edge.clone(),
    }
}

fn rule_names(rules: &[proc_macro2::TokenStream]) -> Vec<valence_core::SchemaPolicyRule> {
    rules
        .iter()
        .map(|ts| valence_core::SchemaPolicyRule {
            name: ts.to_string().replace(' ', ""),
            description: None,
            evaluator: None,
        })
        .collect()
}

fn lower_fields(fields: &[ParsedField]) -> (Vec<SchemaField>, Vec<SchemaEdge>) {
    let mut out_fields = Vec::new();
    let mut edges = Vec::new();

    for field in fields {
        let (enum_variants, enum_type) = enum_meta(&field.field_type);
        let fk = record_table(&field.field_type).map(|ref_table| ForeignKeyRef {
            ref_table: ref_table.to_string(),
            field: "id".to_string(),
        });

        if let Some(ref fk_ref) = fk {
            edges.push(SchemaEdge {
                from_field: field.name.clone(),
                to_table: fk_ref.ref_table.clone(),
                label: edge_label(&field.name),
            });
        }

        out_fields.push(SchemaField {
            name: field.name.clone(),
            field_type: field.field_type.clone(),
            primary: field.primary_key,
            nullable: !field.required,
            indexed: false,
            unique: field.unique,
            default: field.default.clone(),
            fk,
            validations: field.validations.clone(),
            policies: None,
            encrypted: field.encrypted,
            enum_variants,
            enum_type,
            model_path: field.model_path.clone(),
        });
    }

    (out_fields, edges)
}

fn lower_connections(connections: &[ParsedConnection], from_table: &str) -> Vec<SchemaConnection> {
    connections
        .iter()
        .map(|c| SchemaConnection {
            name: c.name.clone(),
            from_table: from_table.to_string(),
            from_field: c.name.clone(),
            to_table: c.table.clone(),
            cardinality: c.cardinality.clone(),
            required: c.required,
            on_delete: c.on_delete.clone(),
            label: connection_display_label(&c.name),
            model_path: c.model.clone(),
            reverse_field: c.reverse_field.clone(),
            edge_table: c.edge_table.clone(),
            target_trait: c.target_trait.clone(),
        })
        .collect()
}

fn enum_meta(field_type: &str) -> (Vec<String>, Option<String>) {
    if let Some(rest) = field_type.strip_prefix("enum:") {
        let variants = rest
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        return (variants, None);
    }
    if let Some(path) = field_type.strip_prefix("ext_enum:") {
        return (Vec::new(), Some(path.to_string()));
    }
    (Vec::new(), None)
}

fn record_table(field_type: &str) -> Option<&str> {
    field_type
        .strip_prefix("record<")
        .and_then(|s| s.strip_suffix('>'))
}

fn edge_label(field_name: &str) -> String {
    field_name
        .strip_suffix("_id")
        .unwrap_or(field_name)
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn connection_display_label(name: &str) -> String {
    name.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
