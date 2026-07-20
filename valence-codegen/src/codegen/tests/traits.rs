#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;
use std::fs;

use crate::codegen::generate_from_schema_file;
use crate::codegen::generate_from_trait_file;
use crate::codegen::parser::extract_trait_from_file;

use super::support::write_temp_schema_file;

#[test]
fn generates_trait_definition() {
    let trait_schema = r"
use valence::prelude::*;

valence_trait_schema! {
    Named {
        fields: [
            name: { r#type: FieldType::String, required: true },
        ],
    }
}
";

    let path = write_temp_schema_file(trait_schema, "named_valence_trait.rs");
    let generated = generate_from_trait_file(&path).expect("trait codegen failed");

    assert!(
        generated.contains("pub trait NamedFields"),
        "Expected NamedFields trait"
    );
    assert!(
        generated.contains("pub struct NamedModel"),
        "Expected NamedModel struct"
    );
    assert!(
        generated.contains("pub trait NamedQuery"),
        "Expected NamedQuery trait"
    );
    assert!(
        generated.contains("pub struct NamedQueryAll"),
        "Expected NamedQueryAll struct"
    );
    assert!(
        generated.contains("fn where_name"),
        "Expected where_name method"
    );
    assert!(
        generated.contains("fn order_by_name"),
        "Expected order_by_name method"
    );
    assert!(
        generated.contains("fn into_parts"),
        "Expected into_parts method"
    );
    assert!(
        generated.contains("fn from_parts"),
        "Expected from_parts method"
    );
    assert!(
        generated.contains("tables_for_trait"),
        "Expected TraitRegistry lookup"
    );

    insta::assert_snapshot!(generated);
}

#[test]
fn generates_schema_with_trait() {
    let trait_schema = r"
use valence::prelude::*;

valence_trait_schema! {
    Named {
        fields: [
            name: { r#type: FieldType::String, required: true },
        ],
    }
}
";

    let schema = r#"
use valence::prelude::*;

valence_schema! {
    TestTraitA {
        table: "test_trait_a",
        version: "0.1.0",

        traits: [Named],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            a_specific: { r#type: FieldType::Integer, required: true },
        ],
    }
}
"#;

    // Build trait definitions map
    let trait_path = write_temp_schema_file(trait_schema, "named_valence_trait.rs");
    let trait_content = fs::read_to_string(&trait_path).expect("read trait file");
    let trait_def = extract_trait_from_file(&trait_content).expect("parse trait file");
    let mut trait_defs = HashMap::new();
    trait_defs.insert(trait_def.name.clone(), trait_def);

    let schema_path = write_temp_schema_file(schema, "test_trait_a_valence_schema.rs");
    let generated = generate_from_schema_file(&schema_path, &trait_defs)
        .expect("schema-with-trait codegen failed");

    // The `name` field should be merged from the trait
    assert!(
        generated.contains("name: String"),
        "Expected merged name field"
    );

    // Trait impl blocks should be generated
    assert!(
        generated.contains("impl NamedFields for TestTraitA"),
        "Expected NamedFields impl"
    );
    assert!(
        generated.contains("impl<'a> NamedQuery<'a> for TestTraitAQuery"),
        "Expected NamedQuery impl"
    );
    assert!(
        generated.contains("fn order_by_name(mut self, direction: valence::SortDirection)"),
        "Expected order_by_name delegate on NamedQuery impl"
    );
    assert!(
        generated.contains("from_parts"),
        "Expected from_parts on query builder"
    );
    assert!(
        generated.contains("into_parts"),
        "Expected into_parts on query builder"
    );

    // Refinement extension trait
    assert!(
        generated.contains("NamedQueryRefineTestTraitA"),
        "Expected refinement trait"
    );
    assert!(
        generated.contains("where_is_test_trait_a"),
        "Expected where_is_test_trait_a method"
    );

    insta::assert_snapshot!(generated);
}

#[test]
fn generates_schema_with_trait_connections() {
    let trait_schema = r#"
use valence::prelude::*;

valence_trait_schema! {
    HasOwner {
        fields: [
            owner: { r#type: FieldType::Record("user"), required: true },
        ],
        connections: [
            owner: {
                table: "user",
                cardinality: HasOne,
                required: true,
                on_delete: Cascade,
                model: "crate::generated::User",
            },
        ],
    }
}
"#;

    let schema = r#"
use valence::prelude::*;

valence_schema! {
    OwnedItem {
        table: "owned_item",
        version: "0.1.0",

        traits: [HasOwner],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            label: { r#type: FieldType::String, required: true },
        ],
    }
}
"#;

    let trait_path = write_temp_schema_file(trait_schema, "has_owner_valence_trait.rs");
    let trait_content = fs::read_to_string(&trait_path).expect("read trait file");
    let trait_def = extract_trait_from_file(&trait_content).expect("parse trait file");

    assert_eq!(
        trait_def.connections.len(),
        1,
        "Trait should have 1 connection"
    );
    assert_eq!(trait_def.connections[0].name, "owner");
    assert_eq!(trait_def.connections[0].from_table, "__trait__");

    let mut trait_defs = HashMap::new();
    trait_defs.insert(trait_def.name.clone(), trait_def);

    let schema_path = write_temp_schema_file(schema, "owned_item_valence_schema.rs");
    let generated = generate_from_schema_file(&schema_path, &trait_defs)
        .expect("schema-with-trait-connections codegen failed");

    assert!(generated.contains("owner"), "Expected merged owner field");
    assert!(
        generated.contains("get_owner"),
        "Expected get_owner connection method"
    );
    assert!(
        generated.contains("owned_item"),
        "Expected from_table to reference owned_item, got:\n{}",
        generated
            .lines()
            .filter(|l| l.contains("from_table"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn generates_schema_with_multiple_traits() {
    let named_trait = r"
use valence::prelude::*;

valence_trait_schema! {
    Named {
        fields: [
            name: { r#type: FieldType::String, required: true },
        ],
    }
}
";

    let tagged_trait = r"
use valence::prelude::*;

valence_trait_schema! {
    Tagged {
        fields: [
            tag: { r#type: FieldType::String, required: false },
        ],
    }
}
";

    let schema = r#"
use valence::prelude::*;

valence_schema! {
    MultiTrait {
        table: "multi_trait",
        version: "0.1.0",

        traits: [Named, Tagged],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            score: { r#type: FieldType::Integer, required: true },
        ],
    }
}
"#;

    let named_path = write_temp_schema_file(named_trait, "named_valence_trait.rs");
    let named_content = fs::read_to_string(&named_path).expect("read named trait");
    let named_def = extract_trait_from_file(&named_content).expect("parse named trait");

    let tagged_path = write_temp_schema_file(tagged_trait, "tagged_valence_trait.rs");
    let tagged_content = fs::read_to_string(&tagged_path).expect("read tagged trait");
    let tagged_def = extract_trait_from_file(&tagged_content).expect("parse tagged trait");

    let mut trait_defs = HashMap::new();
    trait_defs.insert(named_def.name.clone(), named_def);
    trait_defs.insert(tagged_def.name.clone(), tagged_def);

    let schema_path = write_temp_schema_file(schema, "multi_trait_valence_schema.rs");
    let generated =
        generate_from_schema_file(&schema_path, &trait_defs).expect("multi-trait codegen failed");

    // Both trait fields should be merged
    assert!(
        generated.contains("name: String"),
        "Expected merged name field from Named"
    );
    assert!(
        generated.contains("tag"),
        "Expected merged tag field from Tagged"
    );

    // Both trait impls should be generated
    assert!(
        generated.contains("impl NamedFields for MultiTrait"),
        "Expected NamedFields impl"
    );
    assert!(
        generated.contains("impl TaggedFields for MultiTrait"),
        "Expected TaggedFields impl"
    );
    assert!(
        generated.contains("impl<'a> NamedQuery<'a> for MultiTraitQuery"),
        "Expected NamedQuery impl"
    );
    assert!(
        generated.contains("impl<'a> TaggedQuery<'a> for MultiTraitQuery"),
        "Expected TaggedQuery impl"
    );

    // Both refinement traits
    assert!(
        generated.contains("NamedQueryRefineMultiTrait"),
        "Expected Named refinement trait"
    );
    assert!(
        generated.contains("TaggedQueryRefineMultiTrait"),
        "Expected Tagged refinement trait"
    );
}
