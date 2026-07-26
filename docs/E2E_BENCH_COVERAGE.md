# E2E & bench coverage matrix

Living coverage map for Valence. Status legend:

| Symbol | Meaning |
|--------|---------|
| `Y` | Covered |
| `P` | Partial / smoke |
| `N` | Missing |
| `H` | Host-owned (outside this repo) |
| `D` | Deferred by design |

**Target contract:** every single-backend feature E2E row runs on all storage adapters (mem, sqlite, surreal-mem, surreal-rocksdb, indradb, postgres, mongodb, redis; acme-stub where the port applies). Full matrix + benches execute on **AWS** — see [AWS_E2E_BENCH_CAMPAIGN.md](AWS_E2E_BENCH_CAMPAIGN.md). Local `./scripts/gate.sh` stays unit/clippy only.

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
| Cross-backend depth-2 | Y (Cartesian generator) | Y (missing mid-hop) | **bm-v24** |
| Depth 3–4 nested where | Y (chain host) | Y | **bm-v25** |
| OnDelete Restrict | N | N | N |

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
| DAG plan vs live graph | N | N | N |

### Schema extras

TTL, side effects, iters, trait mixin, encrypted fields — mostly `N` / codegen-only; schedule after query+hop program.

## Storage × suite

| Storage | Catalog E2E | Model | Admin | Deletion | Hop Cartesian |
|---------|-------------|-------|-------|----------|---------------|
| mem | Y | Y | Y | Y | Y |
| sqlite | Y | Y | Y | Y | Y |
| surreal-mem | Y | Y | Y | Y | Y |
| surreal-rocksdb | Y (env) | Y (env) | P | P | Y (env) |
| indradb | Y | Y | Y | Y | Y |
| postgres | Y (URL) | Y (URL) | Y (URL) | Y (URL) | Y (URL) |
| mongodb | Y (URI) | Y (URI) | Y (URI) | Y (URI) | Y (URI) |
| redis | Y (URL) | Y (URL) | Y (URL) | Y (URL) | Y (URL) |
| acme-stub | Y | ignored | N | N | excluded |

## Cross-backend hop Cartesian

Engines (exclude acme-stub): mem, sqlite, surreal-mem, surreal-rocksdb, indradb, postgres, mongodb, redis.

- **Depth 2:** directed pairs `E1 ≠ E2` → 56 layouts via `valence_testkit::hops::directed_pairs()`.
- **Depth 3:** representative triples via `hop_triples_representative()`.
- **Depth 4:** Org→Project→Task→Note chain via `examples/hop-chain-model-host`.

Assertions per layout: seed, loaded-model nav, nested `where_*_has_results`, missing mid-hop empty, privacy fail-closed where applicable.

## Bench registry (new)

| ID | Track |
|----|-------|
| bm-v20 | Get-by-id hammer (hot + unique; cache on/off) |
| bm-v21 | Filtered equality query hammer |
| bm-v22 | Full-scan / large-N |
| bm-v23 | Complex query (multi-predicate + ORDER BY + pagination) |
| bm-v24 | Cross-backend hop depth-2 |
| bm-v25 | Nested hop chain depth-3/4 |

Slices: `adapter-minimal`, `write-sweep`, `query-depth`, `overhead`, `read-hammer`, `query-real`, `hop-pairs`, `hop-chains`.

## Quality gates

See plan + [`architectural layering rules`](../architectural layering rules). Per phase: quality review `scan` / `session_start` / `check_rules` / `health` / `session_end`; clippy `-D warnings` on touched crates; no god files (`max_cc=25`, `max_file_sloc=450`).
