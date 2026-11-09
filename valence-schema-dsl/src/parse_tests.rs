//! Unit tests for [`SchemaSpec`] parsing and [`SchemaSpec::to_schema`] lowering.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::manual_let_else)]

use crate::parse::*;

#[test]
fn test_parse_minimal_schema() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                    name: { r#type: FieldType::String, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "foo");
    assert_eq!(parsed.version, "0.1.0");
    assert_eq!(parsed.fields.len(), 2);
}

#[test]
fn test_parse_connections_block() {
    let input = r#"
            Post {
                repository: "https://github.com/unified-field-dev/valence",
                table: "post",
                version: "0.1.0",
                fields: [ id: { r#type: FieldType::String, primary_key: true, required: true } ],
                connections: [
                    author: { table: "user", on_delete: Cascade, model: "my_crate::User" }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "post");
    assert_eq!(parsed.connections.len(), 1);
    assert_eq!(parsed.connections[0].name, "author");
    assert_eq!(parsed.connections[0].table, "user");
    assert_eq!(
        parsed.connections[0].model.as_deref(),
        Some("my_crate::User")
    );
    assert_eq!(parsed.fields.len(), 1);
}

#[test]
fn test_parse_connection_target_alias() {
    let input = r#"
            Post {
                repository: "https://github.com/unified-field-dev/valence",
                table: "post",
                version: "0.1.0",
                fields: [ id: { r#type: FieldType::String, primary_key: true, required: true } ],
                connections: [
                    author: { table: "user", on_delete: Cascade, target: "my_crate::User" }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(
        parsed.connections[0].model.as_deref(),
        Some("my_crate::User")
    );
}

#[test]
fn test_parse_rejects_unknown_key() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                unknown_key: 1,
                fields: []
            }
        "#;
    let result = syn::parse_str::<SchemaSpec>(input);
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected parse error for unknown key"),
    };
    assert!(err.to_string().contains("Unknown schema key"));
}

#[test]
fn test_parse_traits_key() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ],
                traits: [Named, HasFiles]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "foo");
    assert_eq!(parsed.traits, vec!["Named", "HasFiles"]);
}

#[test]
fn test_parse_ttl_block() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                ttl: {
                    seconds: 1800,
                },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    let ttl = parsed.ttl.expect("ttl should parse");
    assert_eq!(ttl.seconds, 1800);
    assert_eq!(ttl.mode, "backend_capability");
}

#[test]
fn test_parse_enum_field_type() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                    phase: {
                        r#type: FieldType::Enum(&["PENDING", "IN_PROGRESS", "COMPLETED"]),
                        required: true,
                    }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.fields.len(), 2);
    assert_eq!(parsed.fields[1].name, "phase");
    assert_eq!(
        parsed.fields[1].field_type,
        "enum:PENDING,IN_PROGRESS,COMPLETED"
    );
}

#[test]
fn test_parse_external_enum_field_type() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true },
                    color: {
                        r#type: FieldType::ExternalEnum("crate::ColorEnum"),
                        required: true,
                    }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.fields.len(), 2);
    assert_eq!(parsed.fields[1].name, "color");
    assert_eq!(parsed.fields[1].field_type, "ext_enum:crate::ColorEnum");
}

#[test]
fn test_parse_connections_optional() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.table_name, "foo");
    assert_eq!(parsed.fields.len(), 1);
    assert!(parsed.connections.is_empty());
}

#[test]
fn test_parse_duplicate_database() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                database: DB_A,
                database: DB_B,
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("duplicate `database:`"));
}

#[test]
fn test_parse_ownership_mutual_exclusion() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                ownership: { system: true, resolve: crate::Resolver },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("ownership:"));
}

#[test]
fn test_parse_ttl_requires_seconds() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                table: "foo",
                version: "0.1.0",
                ttl: { mode: "backend_capability" },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("ttl.seconds"));
}

#[test]
fn test_parse_connection_requires_on_delete() {
    let input = r#"
            Post {
                repository: "https://github.com/unified-field-dev/valence",
                table: "post",
                version: "0.1.0",
                fields: [ id: { r#type: FieldType::String, primary_key: true, required: true } ],
                connections: [
                    author: { table: "user" }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("on_delete"));
}

#[test]
fn test_parse_missing_table() {
    let input = r#"
            Foo {
                repository: "https://github.com/unified-field-dev/valence",
                version: "0.1.0",
                fields: []
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let err = schema.to_schema().unwrap_err();
    assert!(err.to_string().contains("Missing 'table'"));
}

#[test]
fn defer_to_edge_parses_string_edge_happy() {
    let input = r#"
            Hist {
                repository: "https://github.com/unified-field-dev/valence",
                table: "hist",
                version: "0.1.0",
                policies: {
                    read: {
                        defer_to_edge: "source",
                    }
                },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    let read = parsed
        .policies
        .as_ref()
        .expect("policies")
        .read
        .as_ref()
        .expect("read");
    assert_eq!(read.defer_to_edge.as_deref(), Some("source"));
}

#[test]
fn defer_to_edge_unknown_key_sad() {
    let input = r#"
            Hist {
                repository: "https://github.com/unified-field-dev/valence",
                table: "hist",
                version: "0.1.0",
                policies: {
                    read: {
                        not_a_real_bucket: [PUBLIC_READ],
                    }
                },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let schema = syn::parse_str::<SchemaSpec>(input).expect("parse structure");
    let err = schema.to_schema().expect_err("unknown policy key");
    let msg = err.to_string();
    assert!(
        msg.contains("Unknown policy rule key") || msg.contains("not_a_real_bucket"),
        "{msg}"
    );
}

#[test]
fn defer_to_edge_non_string_sad() {
    let input = r#"
            Hist {
                repository: "https://github.com/unified-field-dev/valence",
                table: "hist",
                version: "0.1.0",
                policies: {
                    read: {
                        defer_to_edge: 123,
                    }
                },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let msg = match syn::parse_str::<SchemaSpec>(input) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("non-string defer_to_edge must fail parse"),
    };
    assert!(
        msg.contains("defer_to_edge") || msg.contains("expected") || msg.contains("string"),
        "{msg}"
    );
}

#[test]
fn defer_to_edge_empty_edge_sad() {
    let input = r#"
            Hist {
                repository: "https://github.com/unified-field-dev/valence",
                table: "hist",
                version: "0.1.0",
                policies: {
                    read: {
                        defer_to_edge: "",
                    }
                },
                fields: [
                    id: { r#type: FieldType::String, primary_key: true, required: true }
                ]
            }
        "#;
    let result = syn::parse_str::<SchemaSpec>(input);
    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(schema) => schema.to_schema().expect_err("empty edge").to_string(),
    };
    assert!(
        err_msg.contains("non-empty") || err_msg.contains("defer_to_edge"),
        "{err_msg}"
    );
}

fn retired_schema(retired: &str) -> String {
    format!(
        r#"
            Task {{
                repository: "https://github.com/unified-field-dev/valence",
                table: "task",
                version: "0.2.0",
                fields: [
                    id: {{ r#type: FieldType::String, primary_key: true, required: true }},
                    title: {{ r#type: FieldType::String, required: true }}
                ],
                retired_fields: {retired}
            }}
        "#
    )
}

fn retired_err(retired: &str) -> String {
    let schema = syn::parse_str::<SchemaSpec>(&retired_schema(retired)).expect("parse");
    match schema.to_schema() {
        Ok(_) => panic!("expected retired_fields error for {retired}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn retired_fields_parse_happy() {
    let schema = syn::parse_str::<SchemaSpec>(&retired_schema(r#"["project", "legacy_flag"]"#))
        .expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(parsed.retired_fields, vec!["project", "legacy_flag"]);
    assert_eq!(parsed.fields.len(), 2);
}

#[test]
fn retired_fields_also_in_fields_compile_error_sad() {
    let err = retired_err(r#"["title"]"#);
    assert!(err.contains("still declared in `fields`"), "{err}");
}

#[test]
fn retired_fields_id_compile_error_sad() {
    let err = retired_err(r#"["id"]"#);
    assert!(err.contains("cannot retire `id`"), "{err}");
}

#[test]
fn retired_fields_duplicate_compile_error_sad() {
    let err = retired_err(r#"["project", "project"]"#);
    assert!(err.contains("twice"), "{err}");
}

#[test]
fn retired_fields_invalid_ident_compile_error_sad() {
    let err = retired_err(r#"["not a name"]"#);
    assert!(err.contains("not a valid field identifier"), "{err}");
}

fn edge_policy_schema(connection: &str) -> String {
    format!(
        r#"
            Doc {{
                repository: "https://github.com/unified-field-dev/valence",
                table: "doc",
                version: "0.1.0",
                fields: [
                    id: {{ r#type: FieldType::String, primary_key: true, required: true }},
                    owner: {{ r#type: FieldType::Record("user"), required: true }}
                ],
                connections: [ {connection} ]
            }}
        "#
    )
}

fn edge_policy_err(connection: &str) -> String {
    let schema = syn::parse_str::<SchemaSpec>(&edge_policy_schema(connection)).expect("parse");
    match schema.to_schema() {
        Ok(_) => panic!("expected edge_policy error for {connection}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn edge_policy_parses_on_many_to_many_happy() {
    let schema = syn::parse_str::<SchemaSpec>(&edge_policy_schema(
        r#"labels: { table: "label", cardinality: ManyToMany, edge_table: "doc_label",
                     edge_policy: SourceUpdateTargetRead, on_delete: Cascade }"#,
    ))
    .expect("parse");
    let parsed = schema.to_schema().expect("to_schema");
    assert_eq!(
        parsed.connections[0].edge_policy.as_deref(),
        Some("SourceUpdateTargetRead")
    );
}

#[test]
fn edge_policy_on_has_one_compile_error_sad() {
    let err = edge_policy_err(
        r#"owner: { table: "user", edge_policy: SourceUpdateTargetRead, on_delete: Cascade }"#,
    );
    assert!(
        err.contains("only applies to cardinality ManyToMany"),
        "{err}"
    );
    assert!(err.contains("HasOne"), "{err}");
}

#[test]
fn edge_policy_unknown_value_compile_error_sad() {
    let err = edge_policy_err(
        r#"labels: { table: "label", cardinality: ManyToMany, edge_table: "doc_label",
                     edge_policy: AnyoneMayLink, on_delete: Cascade }"#,
    );
    assert!(err.contains("unknown edge_policy `AnyoneMayLink`"), "{err}");
}

fn spectra_export_schema(block: &str) -> syn::Result<ParsedSchema> {
    let input = format!(
        r#"
            UserCounter {{
                repository: "https://github.com/unified-field-dev/valence",
                table: "user_counter",
                version: "0.1.0",
                {block}
                fields: [
                    id: {{ r#type: FieldType::String, primary_key: true, required: true }}
                ]
            }}
        "#
    );
    syn::parse_str::<SchemaSpec>(&input)?.to_schema()
}

#[test]
fn test_export_block_absent_is_opt_out() {
    assert!(spectra_export_schema("").unwrap().spectra_export.is_none());
}

#[test]
fn test_export_block_explicit_names() {
    let parsed = spectra_export_schema(
        r#"spectra_export: { cadence: Daily, job_name: "user-counter-nightly", target_table: "user_counter_export" },"#,
    )
    .unwrap();
    let cfg = parsed.spectra_export.expect("spectra_export");
    assert_eq!(cfg.cadence, ParsedSpectraExportCadence::Daily);
    assert_eq!(cfg.job_name, "user-counter-nightly");
    assert_eq!(cfg.target_table, "user_counter_export");
}

#[test]
fn test_export_block_defaults_even_when_declared_before_table() {
    let input = r#"
        UserCounter {
            spectra_export: { cadence: ThreeHours },
            repository: "https://github.com/unified-field-dev/valence",
            table: "user_counter",
            version: "0.1.0",
            fields: [ id: { r#type: FieldType::String, primary_key: true, required: true } ]
        }
    "#;
    let parsed = syn::parse_str::<SchemaSpec>(input)
        .unwrap()
        .to_schema()
        .unwrap();
    let cfg = parsed.spectra_export.unwrap();
    assert_eq!(cfg.job_name, "user_counter_three_hours_export");
    assert_eq!(cfg.target_table, "user_counter_export");
}

#[test]
fn test_export_block_every_cadence_parses() {
    for (name, snake) in [
        ("Hourly", "hourly"),
        ("ThreeHours", "three_hours"),
        ("SixHours", "six_hours"),
        ("TwelveHours", "twelve_hours"),
        ("Daily", "daily"),
        ("Weekly", "weekly"),
    ] {
        let cfg = spectra_export_schema(&format!("spectra_export: {{ cadence: {name} }},"))
            .unwrap()
            .spectra_export
            .unwrap();
        assert_eq!(cfg.cadence.variant_name(), name);
        assert_eq!(cfg.job_name, format!("user_counter_{snake}_export"));
    }
}

#[test]
fn test_export_block_unknown_cadence_rejected() {
    let err = spectra_export_schema("spectra_export: { cadence: Monthly },").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Unknown spectra_export cadence `Monthly`"),
        "{msg}"
    );
    assert!(msg.contains("Daily"), "{msg}");
}

#[test]
fn test_export_block_cron_string_rejected() {
    let err = spectra_export_schema(r#"spectra_export: { cadence: "0 0 * * *" },"#).unwrap_err();
    assert!(err.to_string().contains("not a cron string"), "{err}");
}

#[test]
fn test_export_block_sad_paths() {
    for (block, needle) in [
        (
            "spectra_export: { job_name: \"x\" },",
            "cadence is required",
        ),
        (
            "spectra_export: { cadence: Daily, cron: \"x\" },",
            "Unknown spectra_export key",
        ),
        (
            "spectra_export: { cadence: Daily, cadence: Weekly },",
            "Duplicate spectra_export key",
        ),
        (
            "spectra_export: { cadence: Daily, job_name: \"\" },",
            "job_name must be non-empty",
        ),
        (
            "spectra_export: { cadence: Daily, target_table: \"a b\" },",
            "target_table must be non-empty",
        ),
    ] {
        let err = spectra_export_schema(block).unwrap_err();
        assert!(err.to_string().contains(needle), "{block}: {err}");
    }
}
