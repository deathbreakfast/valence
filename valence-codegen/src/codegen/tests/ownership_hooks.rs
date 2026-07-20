#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use crate::codegen::generate_from_schema_file;
use crate::codegen::parser::ParsedTraitDef;

use super::support::write_temp_schema_file;

#[test]
fn create_injects_default_actor_ownership_resolution() {
    let schema = r#"
use valence::prelude::*;
use valence::privacy_policies::common::{PUBLIC_READ, AUTHENTICATED};

valence_schema! {
    OwnershipHookProbe {
        table: "ownership_hook_probe",
        database: valence::DEFAULT_SURREAL_STORAGE,
        version: "0.1.0",
        description: "codegen ownership probe",

        policies: {
            read: { allow: [PUBLIC_READ] },
            create: { allow: [AUTHENTICATED] },
        },

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "ownership_hook_default.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");

    assert!(generated.contains("ensure_active_ownership"));
    assert!(generated.contains("from_actor"));
    assert!(generated.contains("__ensure_ownership_after_write"));
    assert!(generated.contains("ensure_ownership_after_batch_create"));
}

#[test]
fn create_injects_system_owned_when_configured() {
    let schema = r#"
use valence::prelude::*;
use valence::privacy_policies::common::{PUBLIC_READ, AUTHENTICATED, SYSTEM_ONLY};

valence_schema! {
    OwnershipHookSystem {
        table: "ownership_hook_system",
        database: valence::DEFAULT_SURREAL_STORAGE,
        version: "0.1.0",
        description: "system-owned probe",

        ownership: { system: true },

        policies: {
            read: { allow: [PUBLIC_READ] },
            create: { allow: [SYSTEM_ONLY] },
        },

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "ownership_hook_system.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");

    assert!(generated.contains("ensure_active_ownership"));
    assert!(generated.contains("OwnerRef::system()"));
}

#[test]
fn create_injects_custom_resolver_when_configured() {
    let schema = r#"
use valence::prelude::*;
use valence::privacy_policies::common::{PUBLIC_READ, AUTHENTICATED};

valence_schema! {
    OwnershipHookResolve {
        table: "ownership_hook_resolve",
        database: valence::DEFAULT_SURREAL_STORAGE,
        version: "0.1.0",
        description: "custom resolver probe",

        ownership: { resolve: fixture_own_resolver::StubOwnerResolver },

        policies: {
            read: { allow: [PUBLIC_READ] },
            create: { allow: [AUTHENTICATED] },
        },

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "ownership_hook_resolve.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");

    assert!(generated.contains("owner_override()"));
    assert!(generated.contains("fixture_own_resolver::StubOwnerResolver"));
    assert!(generated.contains("resolve_owner"));
    assert!(generated.contains("ensure_active_ownership"));
}

#[test]
fn platform_ownership_tables_skip_ownership_hooks_in_generated_crud() {
    let schema = r#"
use valence::prelude::*;
use valence::privacy_policies::common::{PUBLIC_READ, SYSTEM_ONLY};

valence_schema! {
    OwnCodegenSkipProbe {
        table: "valence_data_ownership",
        database: valence::DEFAULT_SURREAL_STORAGE,
        version: "0.1.0",
        description: "codegen self-skip probe",

        policies: {
            read: { allow: [PUBLIC_READ] },
            create: { allow: [SYSTEM_ONLY] },
            update: { allow: [SYSTEM_ONLY] },
            delete: { allow: [SYSTEM_ONLY] },
        },

        composite_key: [valence_model, record_id],

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            valence_model: { r#type: FieldType::String, required: true },
            record_id: { r#type: FieldType::String, required: true },
            owner_id: { r#type: FieldType::String, required: true },
            owner_type: { r#type: FieldType::Enum(&["user", "system"]), required: true },
            status: { r#type: FieldType::Enum(&["active"]), required: true, default: "active" },
        ]
    }
}
"#;

    let path = write_temp_schema_file(schema, "ownership_hook_skip_platform.rs");
    let generated = generate_from_schema_file(&path, &HashMap::<String, ParsedTraitDef>::new())
        .expect("codegen");

    assert!(
        !generated.contains("valence::ownership::OwnershipService::ensure_active_ownership"),
        "platform ownership table must not inject nested ownership writes"
    );
    assert!(
        !generated.contains("valence::ownership::OwnershipService::mark_pending_deletion"),
        "platform ownership table must not inject delete ownership markers"
    );
}
