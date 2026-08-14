//! OnDelete catalog + cross-engine contracts via [`apply_deletion_node`].

use std::sync::Arc;

use serde_json::json;
use valence_core::actor::Actor;
use valence_core::deletion::apply_deletion_node;
use valence_core::deletion::dag::{DeletionDag, DeletionNode};
use valence_core::evaluator::{Database, DatabaseEvaluator, DEFAULT_IN_MEMORY};
use valence_core::privacy_policies::common::PUBLIC_READ;
use valence_core::query::QueryCore;
use valence_core::record_id::RecordId;
use valence_core::router::DatabaseRouter;
use valence_core::router_key::router_key;
use valence_core::runtime::Valence;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{
    Schema, SchemaConnection, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule,
    SchemaPolicyRules, SchemaPrivacy,
};
use valence_core::trait_registry::TraitRegistry;
use valence_core::DatabaseBackend;

use crate::bootstrap::WireBackendOptions;
use crate::hops::{hop_adapter_excluded, HopPair, HopSkip};
use crate::matrix::{extended_store_available_with_wire, StorageAdapter};
use crate::model_contract::backend_for_storage;
use hop_pair_model_host::{HOP_A, HOP_B};

static OD_PARENT_DB: valence_core::DatabaseFromEngine = Database::from_engine("primary", HOP_A);
static OD_CHILD_DB: valence_core::DatabaseFromEngine = Database::from_engine("secondary", HOP_B);

fn leak_schema(schema: Schema) -> &'static Schema {
    Box::leak(Box::new(schema))
}

fn public_delete_schema(
    name: &str,
    databases: Vec<String>,
    evaluator: &'static dyn DatabaseEvaluator,
    connections: Vec<SchemaConnection>,
) -> &'static SchemaMetadata {
    public_delete_schema_with_fields(name, databases, evaluator, connections, vec![])
}

fn parent_id_field() -> SchemaField {
    SchemaField {
        name: "parent_id".into(),
        field_type: "string".into(),
        primary: false,
        nullable: true,
        indexed: false,
        unique: false,
        default: None,
        fk: None,
        validations: Vec::new(),
        policies: None,
        encrypted: false,
        enum_variants: Vec::new(),
        enum_type: None,
        model_path: None,
    }
}

fn public_delete_schema_with_fields(
    name: &str,
    databases: Vec<String>,
    evaluator: &'static dyn DatabaseEvaluator,
    connections: Vec<SchemaConnection>,
    fields: Vec<SchemaField>,
) -> &'static SchemaMetadata {
    let schema = leak_schema(Schema {
        name: name.to_string(),
        version: "0.1.1".into(),
        databases,
        database_evaluator: evaluator,
        privacy: SchemaPrivacy {
            read: "t".into(),
            write: "t".into(),
        },
        policies: Some(SchemaPolicies {
            delete: Some(SchemaPolicyRules {
                allow: vec![SchemaPolicyRule {
                    name: "PUBLIC".into(),
                    description: None,
                    evaluator: Some(&PUBLIC_READ),
                }],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields,
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

fn has_many_conn(
    name: &str,
    from: &str,
    to: &str,
    reverse: &str,
    on_delete: &str,
) -> SchemaConnection {
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

fn m2m_conn(name: &str, from: &str, to: &str, edge: &str, on_delete: &str) -> SchemaConnection {
    SchemaConnection {
        name: name.into(),
        from_table: from.into(),
        from_field: "id".into(),
        to_table: to.into(),
        cardinality: "ManyToMany".into(),
        required: false,
        on_delete: on_delete.into(),
        label: name.into(),
        model_path: None,
        reverse_field: None,
        edge_table: Some(edge.into()),
        target_trait: None,
    }
}

/// Sort DAG nodes: depth descending, then RemoveEdge → SetNull → CascadeDelete.
pub fn ordered_deletion_nodes(dag: &DeletionDag) -> Vec<&DeletionNode> {
    let max_d = dag.nodes.iter().map(|n| n.depth).max().unwrap_or(0);
    let mut out = Vec::new();
    for d in (0..=max_d).rev() {
        for wave in 0u8..=2 {
            for n in &dag.nodes {
                if n.depth == d && n.action.wave_order() == wave {
                    out.push(n);
                }
            }
        }
    }
    out
}

/// Apply every node in wave order under `valence`.
pub async fn apply_ordered_dag(
    dag: &DeletionDag,
    valence: &Valence,
) -> valence_core::error::Result<()> {
    for node in ordered_deletion_nodes(dag) {
        apply_deletion_node(node, valence).await?;
    }
    Ok(())
}

fn same_backend_registry(kind: SameBackendKind) -> SchemaRegistry {
    let mut reg = SchemaRegistry::new();
    match kind {
        SameBackendKind::Cascade => {
            reg.register(public_delete_schema(
                "od_cascade_parent",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![has_many_conn(
                    "kids",
                    "od_cascade_parent",
                    "od_cascade_child",
                    "parent_id",
                    "Cascade",
                )],
            ));
            reg.register(public_delete_schema(
                "od_cascade_child",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![],
            ));
        }
        SameBackendKind::SetNull => {
            reg.register(public_delete_schema(
                "od_setnull_parent",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![has_many_conn(
                    "kids",
                    "od_setnull_parent",
                    "od_setnull_child",
                    "parent_id",
                    "SetNull",
                )],
            ));
            reg.register(public_delete_schema(
                "od_setnull_child",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![],
            ));
        }
        SameBackendKind::RemoveEdge => {
            reg.register(public_delete_schema(
                "od_edge_parent",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![m2m_conn(
                    "tags",
                    "od_edge_parent",
                    "od_edge_peer",
                    "od_edge_link",
                    "SetNull",
                )],
            ));
            reg.register(public_delete_schema(
                "od_edge_peer",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![],
            ));
        }
        SameBackendKind::Restrict => {
            reg.register(public_delete_schema(
                "od_restrict_parent",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![has_many_conn(
                    "kids",
                    "od_restrict_parent",
                    "od_restrict_child",
                    "parent_id",
                    "Restrict",
                )],
            ));
            reg.register(public_delete_schema(
                "od_restrict_child",
                vec![DEFAULT_IN_MEMORY.name().to_string()],
                &DEFAULT_IN_MEMORY,
                vec![],
            ));
        }
    }
    reg
}

#[derive(Clone, Copy)]
enum SameBackendKind {
    Cascade,
    SetNull,
    RemoveEdge,
    Restrict,
}

/// Same-engine CascadeDelete via compute + apply.
pub async fn run_on_delete_cascade_same_backend(valence: &Valence) -> Result<(), String> {
    let reg = same_backend_registry(SameBackendKind::Cascade);
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    let tag = unique_suffix();
    let pid = format!("p_{tag}");
    let cid = format!("c_{tag}");
    backend
        .create_record("od_cascade_parent", json!({"id": pid, "name": "p"}))
        .await
        .map_err(|e| e.to_string())?;
    backend
        .create_record(
            "od_cascade_child",
            json!({
                "id": cid,
                "parent_id": format!("od_cascade_parent:{pid}")
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    let dag = DeletionDag::compute_with_registry(
        "od_cascade_parent",
        &pid,
        valence,
        &reg,
        &TraitRegistry::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    if !dag.restrict_violations.is_empty() {
        return Err(format!(
            "unexpected Restrict: {:?}",
            dag.restrict_violations
        ));
    }
    apply_ordered_dag(&dag, valence)
        .await
        .map_err(|e| e.to_string())?;

    if QueryCore::get_record_json("od_cascade_child", &cid, valence)
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("cascade child should be gone".into());
    }
    Ok(())
}

/// Same-engine SetNull.
pub async fn run_on_delete_set_null(valence: &Valence) -> Result<(), String> {
    let reg = same_backend_registry(SameBackendKind::SetNull);
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    let tag = unique_suffix();
    let pid = format!("p_{tag}");
    let cid = format!("c_{tag}");
    backend
        .create_record("od_setnull_parent", json!({"id": pid, "name": "p"}))
        .await
        .map_err(|e| e.to_string())?;
    backend
        .create_record(
            "od_setnull_child",
            json!({
                "id": cid,
                "parent_id": format!("od_setnull_parent:{pid}"),
                "name": "keep"
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    let dag = DeletionDag::compute_with_registry(
        "od_setnull_parent",
        &pid,
        valence,
        &reg,
        &TraitRegistry::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    apply_ordered_dag(&dag, valence)
        .await
        .map_err(|e| e.to_string())?;

    let child = QueryCore::get_record_json("od_setnull_child", &cid, valence)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("set-null child should remain")?;
    if !child.get("parent_id").map(|v| v.is_null()).unwrap_or(false) {
        return Err(format!("expected null parent_id, got {child:?}"));
    }
    Ok(())
}

/// Same-engine RemoveEdge (M2M SetNull).
pub async fn run_on_delete_remove_edge(valence: &Valence) -> Result<(), String> {
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    if !backend.capabilities().supports_graph_edges {
        return Ok(());
    }
    let reg = same_backend_registry(SameBackendKind::RemoveEdge);
    let tag = unique_suffix();
    let pid = format!("p_{tag}");
    let tid = format!("t_{tag}");
    backend
        .create_record("od_edge_parent", json!({"id": pid}))
        .await
        .map_err(|e| e.to_string())?;
    backend
        .create_record("od_edge_peer", json!({"id": tid}))
        .await
        .map_err(|e| e.to_string())?;
    let from = RecordId::new("od_edge_parent", &pid);
    let to = RecordId::new("od_edge_peer", &tid);
    valence
        .relate_edge("od_edge_link", &from, &to)
        .await
        .map_err(|e| e.to_string())?;

    let dag = DeletionDag::compute_with_registry(
        "od_edge_parent",
        &pid,
        valence,
        &reg,
        &TraitRegistry::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    apply_ordered_dag(&dag, valence)
        .await
        .map_err(|e| e.to_string())?;

    let targets = backend
        .get_edge_targets(&from, "od_edge_link")
        .await
        .map_err(|e| e.to_string())?;
    if !targets.is_empty() {
        return Err("edges should be cleared".into());
    }
    if QueryCore::get_record_json("od_edge_peer", &tid, valence)
        .await
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("peer row should remain".into());
    }
    Ok(())
}

/// Restrict blocks apply (sad path).
pub async fn run_on_delete_restrict_blocks(valence: &Valence) -> Result<(), String> {
    let reg = same_backend_registry(SameBackendKind::Restrict);
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    let tag = unique_suffix();
    let pid = format!("p_{tag}");
    let cid = format!("c_{tag}");
    backend
        .create_record("od_restrict_parent", json!({"id": pid}))
        .await
        .map_err(|e| e.to_string())?;
    backend
        .create_record(
            "od_restrict_child",
            json!({
                "id": cid,
                "parent_id": format!("od_restrict_parent:{pid}")
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    let dag = DeletionDag::compute_with_registry(
        "od_restrict_parent",
        &pid,
        valence,
        &reg,
        &TraitRegistry::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    if dag.restrict_violations.is_empty() {
        return Err("expected Restrict violations".into());
    }
    if !dag.nodes.is_empty() {
        return Err("Restrict DAG must have empty nodes".into());
    }
    // Do not apply — parent and child must still exist.
    if QueryCore::get_record_json("od_restrict_parent", &pid, valence)
        .await
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("parent must remain when Restrict blocks".into());
    }
    Ok(())
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Representative secondary for OnDelete cross-engine when `primary` is the catalog storage.
#[must_use]
pub fn on_delete_cross_engine_secondary(primary: StorageAdapter) -> Option<StorageAdapter> {
    match primary {
        StorageAdapter::Mem => Some(StorageAdapter::Sqlite),
        StorageAdapter::Sqlite => Some(StorageAdapter::Mem),
        StorageAdapter::Postgres
        | StorageAdapter::MongoDb
        | StorageAdapter::SurrealMem
        | StorageAdapter::SurrealRocksdb
        | StorageAdapter::IndraDb
        | StorageAdapter::HybridIndraPg => Some(StorageAdapter::Sqlite),
        StorageAdapter::Redis => Some(StorageAdapter::Mem),
        StorageAdapter::AcmeStub => None,
    }
}

async fn build_hop_valence(
    pair: HopPair,
    wire: Option<&WireBackendOptions>,
) -> Result<Valence, String> {
    if hop_adapter_excluded(pair.primary) || hop_adapter_excluded(pair.secondary) {
        return Err(format!(
            "SKIP {} — acme-stub excluded",
            HopSkip::BackendUnavailable.label()
        ));
    }
    if !extended_store_available_with_wire(pair.primary, wire)
        || !extended_store_available_with_wire(pair.secondary, wire)
    {
        return Err(format!(
            "SKIP {} — primary/secondary unavailable",
            HopSkip::BackendUnavailable.label()
        ));
    }

    let primary = backend_for_storage(pair.primary, wire)
        .await
        .map_err(|e| e.to_string())?;
    let secondary = backend_for_storage(pair.secondary, wire)
        .await
        .map_err(|e| e.to_string())?;

    let mut router = DatabaseRouter::new();
    router.register(router_key("primary", HOP_A), Arc::clone(&primary));
    router.register(router_key("secondary", HOP_B), Arc::clone(&secondary));

    Valence::builder()
        .database_router(Arc::new(router))
        .default_backend_key(router_key("primary", HOP_A))
        .with_actor(Actor::User {
            user_id: "on_delete_xe".into(),
        })
        .build()
        .map_err(|e| e.to_string())
}

async fn sync_hop_typed_tables(valence: &Valence) -> Result<(), String> {
    for table in [
        "od_xe_ca_parent",
        "od_xe_ca_child",
        "od_xe_sn_parent",
        "od_xe_sn_child",
    ] {
        valence_core::storage_layout::sync_typed_table_for(valence, table)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Cross-engine CascadeDelete (parent primary, child secondary).
pub async fn run_on_delete_cascade_cross_engine(
    pair: HopPair,
    wire: Option<&WireBackendOptions>,
) -> Result<(), String> {
    let valence = match build_hop_valence(pair, wire).await {
        Ok(v) => v,
        Err(msg) if msg.starts_with("SKIP") => {
            eprintln!("on-delete cascade cross-engine {}: {msg}", pair.slug());
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    // Route via global registry: register leaked schemas once per process.
    ensure_cross_engine_schemas_registered();
    sync_hop_typed_tables(&valence).await?;

    let tag = unique_suffix();
    let pid = format!("xp_{tag}");
    let cid = format!("xc_{tag}");
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
    if Arc::ptr_eq(&parent_be, &child_be) {
        return Err("cross-engine fixture must use distinct parent/child backends".into());
    }

    let dag = DeletionDag::compute("od_xe_ca_parent", &pid, &valence)
        .await
        .map_err(|e| e.to_string())?;
    if !dag.restrict_violations.is_empty() {
        return Err(format!(
            "unexpected Restrict: {:?}",
            dag.restrict_violations
        ));
    }
    apply_ordered_dag(&dag, &valence)
        .await
        .map_err(|e| e.to_string())?;

    if QueryCore::get_record_json("od_xe_ca_child", &cid, &valence)
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("cross-engine cascade child should be gone".into());
    }
    Ok(())
}

/// Cross-engine SetNull (FK cleared on secondary).
pub async fn run_on_delete_set_null_cross_engine(
    pair: HopPair,
    wire: Option<&WireBackendOptions>,
) -> Result<(), String> {
    let valence = match build_hop_valence(pair, wire).await {
        Ok(v) => v,
        Err(msg) if msg.starts_with("SKIP") => {
            eprintln!("on-delete set-null cross-engine {}: {msg}", pair.slug());
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    ensure_cross_engine_schemas_registered();
    sync_hop_typed_tables(&valence).await?;

    let tag = unique_suffix();
    let pid = format!("sp_{tag}");
    let cid = format!("sc_{tag}");
    let parent_be = valence
        .backend_for_table("od_xe_sn_parent")
        .map_err(|e| e.to_string())?;
    let child_be = valence
        .backend_for_table("od_xe_sn_child")
        .map_err(|e| e.to_string())?;
    parent_be
        .create_record("od_xe_sn_parent", json!({"id": pid}))
        .await
        .map_err(|e| e.to_string())?;
    child_be
        .create_record(
            "od_xe_sn_child",
            json!({
                "id": cid,
                "parent_id": format!("od_xe_sn_parent:{pid}")
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    let dag = DeletionDag::compute("od_xe_sn_parent", &pid, &valence)
        .await
        .map_err(|e| e.to_string())?;
    apply_ordered_dag(&dag, &valence)
        .await
        .map_err(|e| e.to_string())?;

    let child = QueryCore::get_record_json("od_xe_sn_child", &cid, &valence)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("set-null child should remain on secondary")?;
    if !child.get("parent_id").map(|v| v.is_null()).unwrap_or(false) {
        return Err(format!("expected null FK on secondary, got {child:?}"));
    }
    Ok(())
}

fn ensure_cross_engine_schemas_registered() {
    // Force global registry init so inventory SchemaMetadataInit submissions are visible.
    let reg = SchemaRegistry::global();
    assert!(
        reg.get_schema("od_xe_ca_parent").is_some()
            && reg.get_schema("od_xe_ca_child").is_some()
            && reg.get_schema("od_xe_sn_parent").is_some()
            && reg.get_schema("od_xe_sn_child").is_some(),
        "OnDelete cross-engine schemas missing from SchemaRegistry::global (inventory link?)"
    );
}

// Inventory registration so `DeletionDag::compute` + `backend_for_table` route hop engines.
valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        public_delete_schema(
            "od_xe_ca_parent",
            vec![OD_PARENT_DB.name().to_string()],
            &OD_PARENT_DB,
            vec![has_many_conn(
                "kids",
                "od_xe_ca_parent",
                "od_xe_ca_child",
                "parent_id",
                "Cascade",
            )],
        )
    })
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        public_delete_schema_with_fields(
            "od_xe_ca_child",
            vec![OD_CHILD_DB.name().to_string()],
            &OD_CHILD_DB,
            vec![],
            vec![parent_id_field()],
        )
    })
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        public_delete_schema(
            "od_xe_sn_parent",
            vec![OD_PARENT_DB.name().to_string()],
            &OD_PARENT_DB,
            vec![has_many_conn(
                "kids",
                "od_xe_sn_parent",
                "od_xe_sn_child",
                "parent_id",
                "SetNull",
            )],
        )
    })
}

valence_core::inventory::submit! {
    valence_core::schema::SchemaMetadataInit(|| {
        public_delete_schema_with_fields(
            "od_xe_sn_child",
            vec![OD_CHILD_DB.name().to_string()],
            &OD_CHILD_DB,
            vec![],
            vec![parent_id_field()],
        )
    })
}

/// Soft-skip friendly runner for representative OnDelete hop pairs.
pub async fn run_on_delete_hop_pairs(
    wire: Option<&WireBackendOptions>,
) -> valence_core::error::Result<()> {
    let pairs = [
        HopPair {
            primary: StorageAdapter::Mem,
            secondary: StorageAdapter::Sqlite,
        },
        HopPair {
            primary: StorageAdapter::Sqlite,
            secondary: StorageAdapter::Mem,
        },
        HopPair {
            primary: StorageAdapter::Postgres,
            secondary: StorageAdapter::Sqlite,
        },
        HopPair {
            primary: StorageAdapter::MongoDb,
            secondary: StorageAdapter::Sqlite,
        },
        HopPair {
            primary: StorageAdapter::Redis,
            secondary: StorageAdapter::Mem,
        },
    ];
    for pair in pairs {
        run_on_delete_cascade_cross_engine(pair, wire)
            .await
            .map_err(valence_core::error::Error::Internal)?;
        run_on_delete_set_null_cross_engine(pair, wire)
            .await
            .map_err(valence_core::error::Error::Internal)?;
    }
    Ok(())
}
