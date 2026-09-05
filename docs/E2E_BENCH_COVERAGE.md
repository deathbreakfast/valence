# E2E & bench coverage matrix

Living coverage map for Valence. Status legend:

| Symbol | Meaning |
|--------|---------|
| `Y` | Covered — scenario **executed** with validating assertions |
| `P` | Partial / smoke |
| `N` | Missing |
| `X` | N/A by capability (not a soft-pass) |
| `S` | Soft-skip locally when wire env unset (not coverage) |
| `C` | Campaign / extended CI required (`VALENCE_MATRIX_STRICT=1`) |
| `H` | Host-owned (outside this repo) |
| `D` | Deferred by design |

**Target contract:** every capability-applicable single-backend catalog row runs on all storage adapters. Soft-skip ≠ Y. Extended CI and AWS campaign set `VALENCE_MATRIX_STRICT=1` with Postgres/Redis/Mongo services. Local `./scripts/gate.sh` stays unit/clippy only.

## Feature × Happy / Sad / Bench

### Bootstrap / wiring

| Feature | Happy | Sad | Bench |
|---------|-------|-----|-------|
| Builder + `add_backend` | Y (`builder-smoke`) | Y (`builder-empty-rejects`) | N |
| Multi-logical router | Y | Y (`router-key-not-found`) | N |
| Factory background build | Y | N | N |
| Inventory bootstrap | P (Surreal) | N | N |
| Endpoint env resolve | Y | Y (`endpoint-env-unresolved`) | N |
| Secrets / actor factory | N | N | N (host) |

### Adapter port CRUD

| Feature | Happy | Sad | Bench |
|---------|-------|-----|-------|
| Create/get | Y | Y (`get-record-missing`) | Y bm-v0/v3/v5; **bm-v20** get hammer |
| Update / upsert | Y (contract + model) | P | merge bm-v8; upsert via model contract |
| Hard delete | P (contract) | P | N |
| Unique index | Y (contract) | Y (duplicate) | N |
| Typed ensure / additive sync | Y (`typed-sync-add-field` + `schema_version_sync`) | Y (`schema-version-sqlite-nullability-refuse` + unit refuse) | N |
| Boot version stamp skip/bump | Y (`schema-version-skip` / `schema-version-bump-add-field` + `schema_version_sync`) | Y (stamp mismatch → sync; registry missing-col Validation) | write path per-backend field cache (no every-write catalog inspect once covered) |
| Graph edges | Y | N | N |

### Model / ORM

| Feature | Happy | Sad | Bench |
|---------|-------|-----|-------|
| create/get/merge/delete | Y | N | bm-v6 / bm-v9 |
| update/upsert | Y (`model-update-upsert`) | N | N |
| Read cache hit/miss | P (`read-cache-smoke`) | N | bm-v20 cache on/off |
| Batch create | H | H | H |

### Queries

| Feature | Happy | Sad | Bench |
|---------|-------|-----|-------|
| Empty table query | Y | N | N |
| Filtered WHERE | Y (`query-filter-eq`) | Y (`query-filter-miss`) | **bm-v21** |
| DateTime / Currency / JsonAs round-trip | Y (`typed-field-roundtrip`) | N (serde units cover digit-string) | N |
| DateTimePredicate filter | Y (`query-filter-datetime`) | Y (`query-filter-datetime-miss`) | N |
| ORDER BY | Y (`query-order-by`) | N | **bm-v23** |
| Pagination | Y (`query-pagination`) | Y (`query-offset-empty`) | bm-v14; **bm-v23** |
| Full scan / large-N | P | N | **bm-v22** |
| search / group_by / distinct | N | N | N |
| Union / join builders | Y (`query-union-join-smoke`) | N | N |
| M2M relate/nav | Y (`m2m-relate-smoke`) | N | N |

### Connections / hops

| Feature | Happy | Sad | Bench |
|---------|-------|-----|-------|
| Same-backend HasOne/HasMany | P | N | bm-v15→v24 |
| Cross-backend depth-2 | Y nav (Cartesian) | Y (missing mid-hop) | **bm-v24** |
| Nested EXISTS depth-2..4 | **X** (0.1.x skip) | **X** | **bm-v25** (nav/chain only) |
| OnDelete Restrict | Y (`on-delete-restrict-blocks`) | Y | N |
| OnDelete Cascade / SetNull / RemoveEdge | Y (`on-delete-cascade-same-backend`, `on-delete-set-null`, `on-delete-remove-edge`) | Y (Restrict) | N |
| OnDelete cross-engine | Y (`on-delete-cascade-cross-engine`, `on-delete-set-null-cross-engine`; hop pairs soft-skip without wire) | N | N |

### Privacy / ownership / validation

| Feature | Happy | Sad | Bench |
|---------|-------|-----|-------|
| Privacy deny anonymous | Y | Y | bm-v16/17 |
| Privacy write deny | Y (`privacy-write-deny`) | Y | N |
| Query/list entity privacy post-filter | P (execute path + bm-v27) | P | **bm-v27** |
| Field privacy SYSTEM_ONLY | Y (unit) | Y (`privacy-field-system-only-hidden`) | N (correctness) |
| Dual-key privacy bypass | Y (unit) | Y (`privacy-bypass-requires-force`) | bm-v16/27 (both env keys) |
| Query limit/offset clamp | Y (unit) | Y (`query-limit-clamped`) | N (correctness waiver) |
| Empty policies default-deny | Y (`privacy-empty-default-deny`) | Y | N |
| Upsert update privacy | P (codegen) | P (codegen) | N |
| OWNER_* policies | N | N | N |
| Ownership gate | Y | Y (fail-closed on Err — unit) | N |
| Safe idents / Surreal CQ read-only | Y | Y | N |
| Factory actor bind | Y | Y | N |
| URL credential redaction | Y (unit) | N | N (correctness waiver) |
| Validation reject/accept | Y | Y | N |

### Telemetry / admin / deletion

| Feature | Happy | Sad | Bench |
|---------|-------|-----|-------|
| Recording/console telemetry | Y | N | bm-v2/18 |
| Admin registry/read/delete | Y (all storages via contract) | P | N |
| DeletionService queue | Y | N | bm-v9 |
| Pre-queue DAG Delete privacy (CascadeDelete-only) | Y (`dag_privacy` integ + SetNull filter) | Y (child Deny) | N |
| Synchronous `delete_now` / `delete_entity_now` | Y (`delete_now` integ: cascade, Delete-without-Read, order, id normalize) | Y (privacy deny, Restrict, pending refusal) | **bm-v31** (registered; not executed in this change) |
| Deletion requester actor restore | Y (platform `requester_actor`) | Y (missing `requested_by`) | N |
| DAG plan vs live graph | Y (platform + catalog OnDelete) | Y (Restrict) | N |

### Schema extras

| Feature | Happy | Sad | Notes |
|---------|-------|-----|-------|
| Table TTL (create-only) | Y | Y | Catalog: `ttl-native-expire` (Redis/Mongo), `ttl-deferred-stamp` (Deferred/Unsupported linger without Chronon), `ttl-deferred-sweep-delete` (Deferred adapters: expired row gone), `ttl-create-only-no-refresh`, `ttl-non-native-warn`. Platform budgeted sweeper: `valence-platform` `ttl_sweep_*` / hybrid integ. Bench: **not required**. Mongo purge timing not waited (TTL monitor). |
| Side effects on queued/cascade physical delete | Y (platform TM-V3 cascade-child SE) | Y (Restrict → SE=0) | Platform integ; L0 catalog uses `apply_deletion_node` without Chronon SE |
| Iters / trait mixin / encrypted | Y (`iter-scan-complete` on SQL/Surreal/mem/hybrid) | X Redis/Mongo/Indra/Acme (no keyset pushdown yet) | Platform: `iter_scan_complete` test. Trait mixin / encrypted still N. |

Registered campaign scenario IDs: `ttl-native-expire`, `ttl-deferred-stamp`, `ttl-deferred-sweep-delete`, `ttl-create-only-no-refresh`, `ttl-non-native-warn`,
`iter-scan-complete`,
`typed-field-roundtrip`, `query-filter-datetime`, `query-filter-datetime-miss`,
`on-delete-cascade-same-backend`, `on-delete-set-null`, `on-delete-remove-edge`, `on-delete-restrict-blocks`,
`on-delete-cascade-cross-engine`, `on-delete-set-null-cross-engine`,
`delete-now-cascade`, `delete-now-privacy-deny`, `delete-now-restrict`, `delete-now-cross-engine-partial-retry`
(AcmeStub skipped for OnDelete / TTL / typed-field / delete-now).

## Storage × suite

| Storage | Catalog E2E | Model | Admin | Deletion | Hop Cartesian |
|---------|-------------|-------|-------|----------|---------------|
| mem | Y | Y | Y | Y | Y (nav; nested EXISTS X) |
| sqlite | Y | Y | Y | Y | Y (nav; nested EXISTS X) |
| surreal-mem | Y | Y | Y | Y | Y (nav; nested EXISTS X) |
| surreal-rocksdb | C (`VALENCE_BENCH_ROCKSDB=1`) | C | C | C | C |
| indradb | Y | Y | Y | Y | Y (nav; nested EXISTS X) |
| postgres | C (`DATABASE_URL` + STRICT) | C | C | C | C |
| mongodb | C / S | C / S | C / S | C / S | C / S |
| redis | C / S | C / S | C / S | C / S | C / S |
| hybrid | C (`hybrid` + URL) | C | C | C | C |
| acme-stub | Y (subset) | X | X | X | X |

Typed-field catalog rows (`typed-field-roundtrip`, `query-filter-datetime`, `query-filter-datetime-miss`) run on every capable adapter above (AcmeStub X).

## Cross-backend hop Cartesian

Engines (exclude acme-stub): mem, sqlite, surreal-mem, surreal-rocksdb, indradb, postgres, mongodb, redis.

- **Depth 2:** directed pairs `E1 ≠ E2` → navigation asserted; nested `where_*_has_results` is **X**.
- **Depth 3–4:** representative chains — navigation asserted; nested EXISTS **X**.

Assertions per layout: seed, loaded-model nav, missing mid-hop empty. Nested EXISTS skipped with `nested_where_unsupported`.

## Bench registry (new)

| ID | Track |
|----|-------|
| bm-v20 | Get-by-id hammer (hot + unique; cache on/off) |
| bm-v21 | Filtered equality query hammer |
| bm-v22 | Full-scan / large-N |
| bm-v23 | Complex query (multi-predicate + ORDER BY + pagination) |
| bm-v24 | Cross-backend hop depth-2 |
| bm-v25 | Nested hop chain depth-3/4 |
| bm-v28 | Get-by-id firehose (read ops/s + sampled p95) |
| bm-v29 | Mixed OLTP `prod-mix-v1` (create / hot get / cold primary get / eq filter), one client |
| bm-v30 | Mixed OLTP `prod-mix-v1` per-client tables for N=2/4 app nodes |

Slices: `adapter-minimal`, `write-sweep`, `query-depth`, `overhead`, `read-hammer`, `query-real`, `hop-pairs`, `hop-chains`, `marketing-capacity`, `marketing-mixed`, `marketing-mixed-scale`.

## Quality gates

See plan + [`architectural layering rules`](../architectural layering rules). Per phase: quality review `scan` / `session_start` / `check_rules` / `health` / `session_end`; clippy `-D warnings` on touched crates; no god files (`max_cc=25`, `max_file_sloc=450`).
