//! Declarative scenario steps shared by e2e (assert) and bench (measure).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::matrix::StorageAdapter;

/// One step in a Valence matrix scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum ScenarioStep {
    /// Build [`valence_core::Valence`] from the bootstrapped router.
    BuildValence,
    /// Assert the active backend resolves.
    AssertActiveBackend,
    /// Assert a router key resolves.
    AssertRouterResolve {
        /// Compound router key.
        key: String,
    },
    /// Assert a router key does not resolve (sad path).
    AssertRouterResolveFails {
        /// Compound router key expected to be missing.
        key: String,
    },
    /// Assert minimum router registration count.
    AssertRouterLen {
        /// Minimum expected registrations.
        min: usize,
    },
    /// Assert two router keys resolve to the same backend instance (shared `Arc`).
    AssertRouterSharedBackend {
        /// First compound router key.
        key_a: String,
        /// Second compound router key.
        key_b: String,
    },
    /// Create a record via the backend at `create_key`, read it back via `read_key`.
    ///
    /// Behavioral multi-logical check: both keys must address the same backend.
    CrudAcrossRouterKeys {
        /// Compound router key used for the create.
        create_key: String,
        /// Compound router key used for the read-back.
        read_key: String,
        /// Table name.
        table: String,
        /// Record id.
        id: String,
    },
    /// Create + get a smoke record on the active backend.
    CrudSmoke {
        /// Table name.
        table: String,
        /// Record id.
        id: String,
    },
    /// Assert get on a missing record returns none (sad path).
    AssertGetMissing {
        /// Table name.
        table: String,
        /// Record id that must not exist.
        id: String,
    },
    /// Assert anonymous read is denied on the auth-only fixture schema (sad path).
    AssertPrivacyReadDenied,
    /// Assert anonymous create is denied (sad path).
    AssertPrivacyWriteDenied,
    /// Assert a validation helper rejects a value (sad path).
    AssertValidationRejects {
        /// Validator name (`email`, `non_empty`).
        validator: String,
        /// Input that must fail validation.
        value: String,
    },
    /// Assert a validation helper accepts a value (happy path).
    AssertValidationAccepts {
        /// Validator name (`email`, `non_empty`).
        validator: String,
        /// Input that must pass validation.
        value: String,
    },
    /// Generated model create/get smoke via product-model-host.
    ModelCrudSmoke,
    /// Generated model update + upsert.
    ModelUpdateUpsert,
    /// Ownership pending-deletion gate allows active rows.
    OwnershipGateSmoke,
    /// Relate/unrelate graph edges when the backend supports them.
    GraphEdgeSmoke,
    /// Assert a telemetry counter was recorded (Recording telemetry).
    AssertTelemetryCounter {
        /// Counter metric name.
        name: String,
        /// Label key.
        label_key: String,
        /// Expected label value.
        label_value: String,
        /// Minimum matching increments.
        min_count: u64,
    },
    /// Build [`valence_core::Valence`] via [`valence_core::ValenceFactory`].
    BuildValenceFromFactory {
        /// Actor JSON passed to the factory.
        actor_json: Value,
    },
    /// Set one env var (restored when the session drops).
    SetEnv {
        /// Environment variable name.
        key: String,
        /// Value to set.
        value: String,
    },
    /// Assert [`valence_core::EnvEndpointResolver`] resolves a URL.
    AssertEndpointResolve {
        /// Logical database name.
        logical: String,
        /// Expected URL.
        expect_url: String,
    },
    /// Assert endpoint resolve returns none when unset (sad path).
    AssertEndpointUnresolved {
        /// Logical database name.
        logical: String,
    },
    /// Read-only compiled query on a missing table returns empty.
    CompiledQueryEmpty {
        /// Table name (may not exist).
        table: String,
    },
    /// Empty `Valence::builder` without backends must fail (sad path).
    AssertBuilderEmptyFails,
    /// ORM equality filter returns matching rows.
    QueryFilterEq,
    /// ORM equality filter returns empty on miss (sad path).
    QueryFilterMiss,
    /// ORM order_by ascending.
    QueryOrderBy,
    /// ORM limit/offset page size.
    QueryPagination,
    /// Far offset returns empty page (sad path).
    QueryOffsetEmpty,
    /// Read cache enabled + invalidate still serves from storage.
    ReadCacheSmoke,
    /// QueryCore union_with / join_with IR composition.
    QueryUnionJoinSmoke,
    /// Many-to-many style relate via graph edges when supported.
    M2mRelateSmoke,
}

/// Declarative scenario specification (JSON-serializable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioSpec {
    /// Stable scenario identifier.
    pub id: String,
    /// Ordered steps.
    pub steps: Vec<ScenarioStep>,
}

impl ScenarioSpec {
    pub fn builder_smoke() -> Self {
        Self {
            id: "builder-smoke".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::AssertActiveBackend,
            ],
        }
    }

    pub fn builder_empty_rejects() -> Self {
        Self {
            id: "builder-empty-rejects".into(),
            steps: vec![ScenarioStep::AssertBuilderEmptyFails],
        }
    }

    pub fn router_multi_logical() -> Self {
        Self::router_multi_logical_engine(
            valence_core::KnownEngines::SURREALDB,
            &["default", "billing"],
        )
    }

    pub fn router_multi_logical_acme() -> Self {
        Self::router_multi_logical_engine("acme_stub", &["primary", "vault"])
    }

    pub fn router_multi_logical_mem() -> Self {
        Self::router_multi_logical_engine(valence_backend_mem::ENGINE_ID, &["default", "billing"])
    }

    /// Multi-logical router scenario for any engine id.
    ///
    /// Beyond resolve smoke, when two or more logical names are given this
    /// asserts both keys share one backend instance and that a record created
    /// via the first key reads back via the second (shared-backend behavior),
    /// and that an unregistered logical under the same engine fails to resolve.
    pub fn router_multi_logical_engine(engine_id: &str, logical_names: &[&str]) -> Self {
        let mut steps = vec![
            ScenarioStep::BuildValence,
            ScenarioStep::AssertRouterLen {
                min: logical_names.len(),
            },
        ];
        for name in logical_names {
            steps.push(ScenarioStep::AssertRouterResolve {
                key: valence_core::router_key::router_key(name, engine_id),
            });
        }
        if let [first, second, ..] = logical_names {
            let key_a = valence_core::router_key::router_key(first, engine_id);
            let key_b = valence_core::router_key::router_key(second, engine_id);
            steps.push(ScenarioStep::AssertRouterSharedBackend {
                key_a: key_a.clone(),
                key_b: key_b.clone(),
            });
            steps.push(ScenarioStep::CrudAcrossRouterKeys {
                create_key: key_a,
                read_key: key_b,
                table: "router_multi_smoke".into(),
                id: "rml1".into(),
            });
        }
        steps.push(ScenarioStep::AssertRouterResolveFails {
            key: valence_core::router_key::router_key("nonexistent_logical", engine_id),
        });
        Self {
            id: "router-multi-logical".into(),
            steps,
        }
    }

    pub fn router_key_not_found(storage: StorageAdapter) -> Self {
        let key = crate::fixtures::invalid_router_key(storage.slug());
        Self {
            id: "router-key-not-found".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::AssertRouterResolveFails { key },
            ],
        }
    }

    pub fn get_record_missing() -> Self {
        Self {
            id: "get-record-missing".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::AssertGetMissing {
                    table: "missing_smoke_table".into(),
                    id: "ghost_record".into(),
                },
            ],
        }
    }

    pub fn privacy_read_deny_anonymous() -> Self {
        Self {
            id: "privacy-read-deny-anonymous".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::AssertPrivacyReadDenied,
            ],
        }
    }

    pub fn privacy_write_deny() -> Self {
        Self {
            id: "privacy-write-deny".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::AssertPrivacyWriteDenied,
            ],
        }
    }

    pub fn inventory_bootstrap() -> Self {
        Self {
            id: "inventory-bootstrap".into(),
            steps: vec![
                ScenarioStep::AssertRouterLen { min: 1 },
                ScenarioStep::BuildValence,
                ScenarioStep::AssertActiveBackend,
            ],
        }
    }

    pub fn telemetry_crud_counters() -> Self {
        Self {
            id: "telemetry-crud-counters".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::CrudSmoke {
                    table: "telemetry_smoke".into(),
                    id: "t1".into(),
                },
                ScenarioStep::AssertTelemetryCounter {
                    name: "valence_db_writes".into(),
                    label_key: "op".into(),
                    label_value: "create".into(),
                    min_count: 1,
                },
            ],
        }
    }

    pub fn telemetry_console_smoke() -> Self {
        Self {
            id: "telemetry-console-smoke".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::CrudSmoke {
                    table: "telemetry_console_smoke".into(),
                    id: "c1".into(),
                },
            ],
        }
    }

    pub fn factory_background_build() -> Self {
        Self {
            id: "factory-background-build".into(),
            steps: vec![ScenarioStep::BuildValenceFromFactory {
                actor_json: crate::fixtures::smoke_actor_json(),
            }],
        }
    }

    pub fn endpoint_env_resolve() -> Self {
        Self {
            id: "endpoint-env-resolve".into(),
            steps: vec![
                ScenarioStep::SetEnv {
                    key: "VALENCE_ENDPOINT_DEFAULT".into(),
                    value: "http://127.0.0.1:8000".into(),
                },
                ScenarioStep::AssertEndpointResolve {
                    logical: "default".into(),
                    expect_url: "http://127.0.0.1:8000".into(),
                },
            ],
        }
    }

    pub fn endpoint_env_unresolved() -> Self {
        Self {
            id: "endpoint-env-unresolved".into(),
            steps: vec![ScenarioStep::AssertEndpointUnresolved {
                logical: "no_such_logical".into(),
            }],
        }
    }

    pub fn compiled_query_empty_table() -> Self {
        Self {
            id: "compiled-query-empty-table".into(),
            steps: vec![
                ScenarioStep::BuildValence,
                ScenarioStep::CompiledQueryEmpty {
                    table: "missing_valence_table".into(),
                },
            ],
        }
    }

    pub fn model_crud_smoke() -> Self {
        Self {
            id: "model-crud-smoke".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::ModelCrudSmoke],
        }
    }

    pub fn model_update_upsert() -> Self {
        Self {
            id: "model-update-upsert".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::ModelUpdateUpsert],
        }
    }

    pub fn ownership_gate_smoke() -> Self {
        Self {
            id: "ownership-gate-smoke".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::OwnershipGateSmoke],
        }
    }

    pub fn validation_reject_smoke() -> Self {
        Self {
            id: "validation-reject-smoke".into(),
            steps: vec![ScenarioStep::AssertValidationRejects {
                validator: "email".into(),
                value: "not-an-email".into(),
            }],
        }
    }

    pub fn validation_accept_smoke() -> Self {
        Self {
            id: "validation-accept-smoke".into(),
            steps: vec![ScenarioStep::AssertValidationAccepts {
                validator: "email".into(),
                value: "user@example.com".into(),
            }],
        }
    }

    pub fn graph_edge_smoke() -> Self {
        Self {
            id: "graph-edge-smoke".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::GraphEdgeSmoke],
        }
    }

    pub fn query_filter_eq() -> Self {
        Self {
            id: "query-filter-eq".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::QueryFilterEq],
        }
    }

    pub fn query_filter_miss() -> Self {
        Self {
            id: "query-filter-miss".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::QueryFilterMiss],
        }
    }

    pub fn query_order_by() -> Self {
        Self {
            id: "query-order-by".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::QueryOrderBy],
        }
    }

    pub fn query_pagination() -> Self {
        Self {
            id: "query-pagination".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::QueryPagination],
        }
    }

    pub fn query_offset_empty() -> Self {
        Self {
            id: "query-offset-empty".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::QueryOffsetEmpty],
        }
    }

    pub fn read_cache_smoke() -> Self {
        Self {
            id: "read-cache-smoke".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::ReadCacheSmoke],
        }
    }

    pub fn query_union_join_smoke() -> Self {
        Self {
            id: "query-union-join-smoke".into(),
            steps: vec![ScenarioStep::QueryUnionJoinSmoke],
        }
    }

    pub fn m2m_relate_smoke() -> Self {
        Self {
            id: "m2m-relate-smoke".into(),
            steps: vec![ScenarioStep::BuildValence, ScenarioStep::M2mRelateSmoke],
        }
    }
}
