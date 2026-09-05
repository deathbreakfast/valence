//! Delete-now catalog scenarios (sync `delete_entity_now` path).

use std::sync::Arc;

use serde_json::json;
use valence_core::actor::Actor;
use valence_core::deletion::dag::DeletionAction;
use valence_core::deletion::{
    apply_deletion_node, delete_entity_now, prepare_deletion, DeletionMode, PreparedDeletion,
};
use valence_core::evaluator::{DatabaseEvaluator, DEFAULT_IN_MEMORY};
use valence_core::privacy::PrivacyRule;
use valence_core::privacy_policies::common::{PUBLIC_READ, SYSTEM_ONLY};
use valence_core::query::QueryCore;
use valence_core::router::DatabaseRouter;
use valence_core::router_key::router_key;
use valence_core::runtime::Valence;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaConnection, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};

use crate::bootstrap::WireBackendOptions;
use crate::hops::{hop_adapter_excluded, HopPair, HopSkip};
use crate::matrix::{extended_store_available_with_wire, StorageAdapter};
use crate::model_contract::backend_for_storage;
use crate::on_delete::{
    on_delete_cross_engine_secondary, run_on_delete_cascade_same_backend,
    run_on_delete_restrict_blocks,
};
use hop_pair_model_host::{HOP_A, HOP_B};

fn leak_schema(schema: Schema) -> &'static Schema {
    Box::leak(Box::new(schema))
}

fn has_many(name: &str, from: &str, to: &str, reverse: &str, on_delete: &str) -> SchemaConnection {
    SchemaConnection {
        name: name.into(),
        from_table: from.into(),
        from_field: "id".into(),
        to_table: to.into(),
        cardinality: "HasMany".into(),
        required: false,
        on_delete: on_delete.into(),
        label: name.into(),
        model_path: None,
        reverse_field: Some(reverse.into()),
        edge_table: None,
        target_trait: None,
    }
}

fn schema_with_delete(
    name: &str,
    connections: Vec<SchemaConnection>,
    delete_eval: &'static PrivacyRule,
    delete_name: &str,
) -> &'static SchemaMetadata {
    let schema = leak_schema(Schema {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "t".into(),
            write: "t".into(),
        },
        policies: Some(SchemaPolicies {
            delete: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: delete_name.into(),
                    description: None,
                    evaluator: Some(delete_eval),
                }],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![],
        edges: Vec::new(),
        connections,
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "1d".into(),
            row_count: 0,
            owner: "t".into(),
            description: None,
        },
    });
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn ensure_privacy_schemas() {
    let reg = SchemaRegistry::global();
    assert!(
        reg.get_schema("dncat_priv_p").is_some() && reg.get_schema("dncat_priv_c").is_some(),
        "delete-now privacy schemas missing from SchemaRegistry::global"
    );
}

fn ensure_cross_engine_schemas() {
    let reg = SchemaRegistry::global();
    assert!(
        reg.get_schema("od_xe_ca_parent").is_some() && reg.get_schema("od_xe_ca_child").is_some(),
        "OnDelete cross-engine schemas missing (needed by delete-now partial-retry)"
    );
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        schema_with_delete(
            "dncat_priv_p",
            vec![has_many(
                "kids",
                "dncat_priv_p",
                "dncat_priv_c",
                "parent_id",
                "Cascade",
            )],
            &PUBLIC_READ,
            "PUBLIC",
        )
    })
}
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        schema_with_delete("dncat_priv_c", vec![], &SYSTEM_ONLY, "SYSTEM_ONLY")
    })
}

/// Happy path: sync cascade (delegates to OnDelete same-backend runner).
pub async fn run_delete_now_cascade(valence: &Valence) -> Result<(), String> {
    run_on_delete_cascade_same_backend(valence).await
}

/// Sad path: child Delete privacy deny; no mutation.
pub async fn run_delete_now_privacy_deny(valence: &Valence) -> Result<(), String> {
    ensure_privacy_schemas();
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    let tag = unique_suffix();
    let pid = format!("pp_{tag}");
    let cid = format!("pc_{tag}");
    backend
        .create_record("dncat_priv_p", json!({"id": pid}))
        .await
        .map_err(|e| e.to_string())?;
    backend
        .create_record(
            "dncat_priv_c",
            json!({
                "id": cid,
                "parent_id": format!("dncat_priv_p:{pid}")
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    let err = match delete_entity_now("dncat_priv_p", &pid, valence).await {
        Ok(()) => return Err("expected privacy denial".into()),
        Err(e) => e,
    };
    if !err.to_string().contains("dncat_priv_c") {
        return Err(format!("expected privacy denial naming child, got {err}"));
    }
    if QueryCore::get_record_json("dncat_priv_p", &pid, valence)
        .await
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("parent must remain after privacy deny".into());
    }
    if QueryCore::get_record_json("dncat_priv_c", &cid, valence)
        .await
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("child must remain after privacy deny".into());
    }
    Ok(())
}

/// Sad path: Restrict blocks (delegates to OnDelete restrict runner).
pub async fn run_delete_now_restrict(valence: &Valence) -> Result<(), String> {
    run_on_delete_restrict_blocks(valence).await
}

/// Cross-engine: apply all but root, then `delete_entity_now` completes idempotently.
pub async fn run_delete_now_cross_engine_partial_retry(
    pair: HopPair,
    wire: Option<&WireBackendOptions>,
) -> Result<(), String> {
    if hop_adapter_excluded(pair.primary) || hop_adapter_excluded(pair.secondary) {
        eprintln!(
            "delete-now partial-retry {}: SKIP {}",
            pair.slug(),
            HopSkip::BackendUnavailable.label()
        );
        return Ok(());
    }
    if !extended_store_available_with_wire(pair.primary, wire)
        || !extended_store_available_with_wire(pair.secondary, wire)
    {
        eprintln!(
            "delete-now partial-retry {}: SKIP unavailable backends",
            pair.slug()
        );
        return Ok(());
    }

    let primary = backend_for_storage(pair.primary, wire)
        .await
        .map_err(|e| e.to_string())?;
    let secondary_be = backend_for_storage(pair.secondary, wire)
        .await
        .map_err(|e| e.to_string())?;
    let mut router = DatabaseRouter::new();
    router.register(router_key("primary", HOP_A), Arc::clone(&primary));
    router.register(router_key("secondary", HOP_B), Arc::clone(&secondary_be));
    let valence = Valence::builder()
        .database_router(Arc::new(router))
        .default_backend_key(router_key("primary", HOP_A))
        .with_actor(Actor::User {
            user_id: "delete_now_xe".into(),
        })
        .build()
        .map_err(|e| e.to_string())?;

    ensure_cross_engine_schemas();
    for table in ["od_xe_ca_parent", "od_xe_ca_child"] {
        valence_core::storage_layout::sync_typed_table_for(&valence, table)
            .await
            .map_err(|e| e.to_string())?;
    }

    let tag = unique_suffix();
    let pid = format!("dnp_{tag}");
    let cid = format!("dnc_{tag}");
    let parent_be = valence
        .backend_for_table("od_xe_ca_parent")
        .map_err(|e| e.to_string())?;
    let child_be = valence
        .backend_for_table("od_xe_ca_child")
        .map_err(|e| e.to_string())?;
    parent_be
        .create_record("od_xe_ca_parent", json!({"id": pid}))
        .await
        .map_err(|e| e.to_string())?;
    child_be
        .create_record(
            "od_xe_ca_child",
            json!({
                "id": cid,
                "parent_id": format!("od_xe_ca_parent:{pid}")
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    let PreparedDeletion::Ready { dag, .. } =
        prepare_deletion("od_xe_ca_parent", &pid, DeletionMode::Now, &valence)
            .await
            .map_err(|e| e.to_string())?
    else {
        return Err("expected Ready DAG for partial retry".into());
    };

    let root_idx = dag
        .nodes
        .iter()
        .position(|n| {
            n.table == "od_xe_ca_parent"
                && matches!(n.action, DeletionAction::CascadeDelete)
                && n.record_id == pid
        })
        .ok_or("root cascade node missing")?;

    for (i, node) in dag.nodes.iter().enumerate() {
        if i == root_idx {
            continue;
        }
        apply_deletion_node(node, &valence)
            .await
            .map_err(|e| e.to_string())?;
    }

    if QueryCore::get_record_json("od_xe_ca_child", &cid, &valence)
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("child should be gone after partial apply".into());
    }
    if QueryCore::get_record_json("od_xe_ca_parent", &pid, &valence)
        .await
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("root must remain until retry".into());
    }

    delete_entity_now("od_xe_ca_parent", &pid, &valence)
        .await
        .map_err(|e| e.to_string())?;
    if QueryCore::get_record_json("od_xe_ca_parent", &pid, &valence)
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("root should be gone after idempotent retry".into());
    }
    Ok(())
}

/// Soft-skip friendly runner when catalog storage has a cross-engine secondary.
pub async fn run_delete_now_partial_retry_for_storage(
    storage: StorageAdapter,
    wire: Option<&WireBackendOptions>,
) -> Result<(), String> {
    let Some(secondary) = on_delete_cross_engine_secondary(storage) else {
        return Ok(());
    };
    run_delete_now_cross_engine_partial_retry(
        HopPair {
            primary: storage,
            secondary,
        },
        wire,
    )
    .await
}
