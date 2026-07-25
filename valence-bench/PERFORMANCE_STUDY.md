# Valence performance study

Executive summary of Valence ORM / storage-adapter capacity experiments. Methodology uses firehose load, multi-client aggregate (bc), metric tracks, and prefill-before-measure.

**Scope:** Valence only (`valence-bench`).

## Research questions

| RQ | Question | Experiment |
|----|----------|------------|
| RQ-VW1 | Single-client sustained create ops/s per adapter? | bm-v5 |
| RQ-VW2 | Concurrency saturation (error rate < 0.1%)? | bm-v5 `--concurrency` sweep |
| RQ-VW3 | Aggregate write ops/s with bc clients? | bm-v7 + `scripts/bench-aggregate-bc.sh` |
| RQ-VW4 | Valence runtime vs raw adapter create? | bm-v0 vs bm-v5 vs bm-v6 |
| RQ-VW5 | Merge/update and soft-delete queue cost? | bm-v8, bm-v9 |
| RQ-VQ1 | Compiled query latency vs depth N? | bm-v11 |
| RQ-VQ2 | ORM query latency vs depth? | bm-v12 |
| RQ-VQ3 | Filter shape overhead? | bm-v13 |
| RQ-VQ4 | Pagination limit/offset at depth? | bm-v14 |
| RQ-VQ5 | Connection hop / inner query? | bm-v15 → **bm-v24/v25** |
| RQ-VQ6 | Cross-adapter query compare? | bm-v11/bm-v12 matrix |
| RQ-VO1 | Privacy read gate on vs bypass? | bm-v16 |
| RQ-VO2 | Privacy eval sleep sensitivity? | bm-v17 |
| RQ-VO5 | ORM query privacy post-filter cost vs bypass? | **bm-v27** (`privacy-filter` slice) |
| RQ-VO3 | Telemetry recording vs off? | bm-v18 (mem-only) |
| RQ-VO4 | CPU/RSS loading 10k rows? | bm-v19 |
| RQ-VR1 | Get-by-id hammer / cache? | **bm-v20** |
| RQ-VQ7 | Filtered / complex / full-scan? | **bm-v21/v22/v23** |
| RQ-VI1 | Secondary index vs full scan on filter shapes? | *future* (see below) |
| RQ-VI2 | Index type matrix (B-tree / hash / unique / partial / JSON / FTS)? | *future* |
| RQ-VI3 | Write-path cost of maintaining indexes? | *future* |
| RQ-VI4 | When does an index stop helping (selectivity / prefill)? | *future* |
| RQ-VC1 | Hybrid IndraDB cache + Postgres primary (get / query / hop)? | **bm-v26** |
| RQ-VC2 | Distributed shared cache under multi-client load? | *future* |
| RQ-VC3 | Cache invalidation / TTL vs stale reads and write amp? | *future* |
| RQ-VC4 | Index + cache together vs either alone? | *future* |

**bm-v9** measures valence-core soft-delete queue path (`queue_delete_entity`) — not platform cascade execution.

## Adapter matrix

| Adapter | Write | Query | Local (no URL) | Remote URL |
|---------|-------|-------|----------------|------------|
| mem | yes | yes | yes | — |
| sqlite | yes | yes | yes | — |
| surreal-mem | yes | yes | yes | — |
| indradb | yes | yes | yes | — |
| mongodb | yes | yes | skip | `--mongodb-uri` or `VALENCE_MONGODB_URI` |
| redis | yes | yes | skip | `--redis-url` or `VALENCE_REDIS_URL` |
| postgres | yes | yes | skip | `--postgres-url` or `DATABASE_URL` |
| acme-stub | smoke | skip | yes | — |

Reports tag `bench_topology`: `embedded` or `remote`.

## Metric tracks (do not mix)

| Track | Primary metric |
|-------|----------------|
| Write capacity | `achieved_write_ops_per_sec`, `error_rate` |
| Query capacity | `query_ms` p50/p95/p99 |
| Overhead | delta ms vs baseline |

## Run discipline

```bash
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=target-valence-bench
export VALENCE_BENCH_HARDWARE=dev-wsl

cargo run -p valence-bench -- run --experiment bm-v5 --storage redis \
  --redis-url redis://127.0.0.1:6379 --concurrency 32 --duration-secs 30

cargo run -p valence-bench -- matrix adapter-minimal --storage mem,sqlite
```

Wire adapters (mongo/redis/postgres) skip when the builder cannot `resolve()`. Pass explicit URLs via CLI (`--redis-url`, `--mongodb-uri`, `--postgres-url`, `--redis-urls`) or set env vars; bootstrap applies `from_env_defaults()` for unset fields. AWS Redis cluster bc campaigns use `--redis-urls` — deferred until infra quota frees.

Matrix hygiene:

- Mem-only experiments (`bm-v1`, `bm-v2`, `bm-v18`) are **skipped** for non-mem storages (no error stubs).
- `query-real` cells (`bm-v21`–`bm-v23`) use a **120s wall-clock timeout** so full-scan adapters cannot hang the matrix indefinitely.

## Baselines (AWS `c6i.xlarge`)

Source JSON: `profiling/valence-bench/reports/aws/`. Topology: two-host campaign, Docker wire services co-located, **debug builds** — comparative ranking only, not product SLOs.

### What these numbers answer

| Question | Status |
|----------|--------|
| Relative ranking (which adapter is faster for which op on one client) | **Partial** — create+get, write firehose, get-by-id, compiled/ORM query, some filters |
| Single-client / single-backend ceilings | **Not yet** — needs release builds + RQ-VW2 concurrency sweeps |
| How many clients one DB can handle / dead ceilings | **Not yet** — needs RQ-VW2 + RQ-VW3 (see Capacity ceiling campaign) |
| Demand projection / how to scale each backend | **Not yet** — needs saturation curves first |

### Create+get (bm-v0) — op p95 ms

| Storage | p95 ms |
|---------|--------|
| mem | 0.008 |
| indradb | 0.033 |
| sqlite | 0.538 |
| redis | 0.871 |
| mongodb | 1.387 |
| postgres | 2.807 |
| surreal-mem | 3.680 |
| surreal-rocksdb | 12.671 |

### Write firehose (bm-v5) — ops/s

| Storage | ops/s |
|---------|-------|
| mem | 98599 |
| indradb | 51325 |
| redis | 18105 |
| sqlite | 10053 |
| mongodb | 6165 |
| postgres | 2664 |
| surreal-mem | 1628 |
| surreal-rocksdb | 1185 |

### Compiled query @10k (bm-v11) / ORM query @1k (bm-v12) — query p95 ms

| Storage | bm-v11 p95 | bm-v12 p95 |
|---------|------------|------------|
| sqlite | 1.18 | 10.52 |
| postgres | 2.06 | 16.07 |
| mongodb | 2.30 | 16.54 |
| surreal-rocksdb | 2.75 | 18.24 |
| surreal-mem | 3.36 | 18.60 |
| mem | 14.56 | 3.70 |
| redis | 58.05 | 251.86 |
| indradb | 67.40 | 8.66 |

### Get-by-id (bm-v20) — hot get p95 ms (cache-off path in notes)

| Storage | p95 ms |
|---------|--------|
| mem | 0.002 |
| indradb | 0.008 |
| redis | 0.273 |
| sqlite | 0.274 |
| mongodb | 0.618 |
| postgres | 0.757 |
| surreal-mem | 1.275 |
| surreal-rocksdb | 4.226 |

### Filtered / scan / complex (bm-v21–23) — query p95 ms (ok rows)

| Storage | bm-v21 eq | bm-v22 scan | bm-v23 complex |
|---------|-----------|-------------|----------------|
| sqlite | 6.15 | 106.20 | 16.19 |
| surreal-mem | 29.35 | 191.16 | *(was emit bug; fixed)* |
| mem | 33.47 | 37.56 | 179.01 |
| indradb | 92.84 | 91.08 | 240.04 |
| mongodb | 157.55 | 305.38 | 1.26 |
| postgres | *(was dialect bug; fixed)* | 299.44 | *(was dialect bug; fixed)* |

Redis / surreal-rocksdb query-real cells were abandoned when full-scan hung in debug; matrix now times out those cells instead of blocking.

### Hop pairs (bm-v24) — wall p95 ms (partial matrix)

| Pair note | p95 ms |
|-----------|--------|
| mem→indradb | 0.46 |
| mem→sqlite | ~2.67 |
| mem→surreal-mem | 12.21 |
| mem→postgres | 136.60 |

## Error taxonomy (historical campaign stubs)

| Kind | Example | Backend nature? | Resolution |
|------|---------|-----------------|------------|
| Harness / experiment design | bm-v18 non-mem `status=error` | No | Matrix skips mem-only experiments |
| Dialect / adapter bug | postgres `json_extract` on jsonb | No | `rewrite_json_extract_for_postgres` on prepare path |
| Compiler ↔ engine mismatch | surreal-mem SQL `LIKE` | Partly | Surreal emit uses `string::starts_with` / `ends_with` |
| Incomplete / hang | redis/rocksdb @10k full scan | Partly | 120s query-real timeout; pushdown still future |

E2E correctness gaps from the same campaign (Mongo/Redis WHERE no-op; hop `EXISTS` missing child table on Postgres) are fixed in adapters / hop harness — not treated as capacity ceilings.

## Capacity ceiling campaign (planned)

Answers “how many clients can one database handle?” and “what are dead ceilings on this AWS hardware?”

| Question | Experiment | Success signal |
|----------|------------|----------------|
| Single-process concurrency knee | bm-v5 `--concurrency` sweep (1…128) | Peak `achieved_write_ops_per_sec` before `error_rate` ≥ 0.1% or p95 cliff |
| Multi-client aggregate on one DB | bm-v7 + `scripts/bench-aggregate-bc.sh` | Plateau of aggregate ops/s vs client count |
| Read-path ceiling (optional) | bm-v20 / bm-v21 rising concurrency | Same knee metrics for gets/filters |

**Constraints:** `--release` only for ceiling claims; hardware tag `c6i.xlarge` (or sized-up box); prefer one DB service per run; fixed duration (30–60s) per point; abort sweep when error_rate or latency stop rules trip. See [AWS_E2E_BENCH_CAMPAIGN.md](../docs/AWS_E2E_BENCH_CAMPAIGN.md).

Deliverable: saturation curves + per-adapter dead-ceiling table (max useful concurrency, max useful clients, peak ops/s, failure mode).

## Future work — indexes and caching

Not implemented yet. Explore **after** release ceiling curves exist so levers are not tuned against debug noise.

### Indexes (RQ-VI*)

| ID | Question | Likely shape later |
|----|----------|--------------------|
| RQ-VI1 | How much does a secondary index improve filtered query latency vs full scan (bm-v21/v23 shapes)? | Same prefill + filter; index off/on |
| RQ-VI2 | How do index types compare (B-tree / hash / unique / partial / JSON path / full-text)? | Predicate × index type × storage |
| RQ-VI3 | Write-path cost of maintaining those indexes? | bm-v5 / bm-v0 indexed vs unindexed |
| RQ-VI4 | When does an index stop helping (selectivity, cardinality, prefill)? | Prefill sweep × hit-rate |

### Caching (RQ-VC*)

| ID | Question | Likely shape later |
|----|----------|--------------------|
| RQ-VC1 | Hybrid IndraDB+Postgres vs postgres vs indradb (get / query / hop)? | **bm-v26** / `hybrid-compare` slice — AWS `--release` |
| RQ-VC2 | Distributed shared cache under multi-client load? | bm-v7-style clients + shared Redis/Memcached in front of one DB |
| RQ-VC3 | Invalidation / TTL vs stale reads and write amplification? | Get hammer + merge/delete under TTL modes |
| RQ-VC4 | Index + cache together vs either alone on filtered queries? | Factorial: bare / index / cache / both |

### bm-v26 hybrid hypothesis (AWS `--release`)

| Track | Expected winner | Hybrid target |
|-------|-----------------|---------------|
| Get-by-id p95 | indradb | near-indradb (IndraDB mirror) |
| Compiled query p95 | postgres | near-postgres (SQL passthrough) |
| M2M hop / edge fan-out p95 | indradb | near-indradb (mirror edges) |

Numbers land below after the AWS campaign (`scripts/aws-e2e-bench.sh --bench hybrid-compare`).

### Hybrid compare (bm-v26) — AWS `c6i.xlarge` `--release`

Source: `profiling/valence-bench/reports/aws/bm-v26-*.json` (2026-07-21 campaign).

| Storage | get p95 ms | query p95 ms | hop p95 ms |
|---------|------------|--------------|------------|
| hybrid | **0.003** | **0.519** | **0.252** |
| postgres | 0.492 | 0.525 | 16.796 |
| indradb | 0.002 | 13.453 | 0.087 |

**Verdict:** hypothesis holds — hybrid get/hop track near IndraDB; compiled query tracks Postgres (~67× faster than IndraDB on this cell). Hop fan-out is ~67× faster than Postgres on the same host.

### Escalation triggers — when to consider these

Consider indexes / cache only after a Phase-B ceiling curve for the adapter, when one or more hold:

- **Filtered / complex query p95** high while get-by-id is fine → prefer **indexes** (or filter pushdown) before a cache
- **Hot-key get** saturates DB CPU/network with a small working set → prefer **dedicated / distributed cache**
- **Write firehose** drops sharply after indexes → revisit index set (partial/covering) or accept the trade
- **Multi-client aggregate** plateaus with high DB CPU and low cache hit rate → cache or read replicas; if DB CPU low and client-bound → more clients will not help
- **Cross-backend hops** dominate wall time → indexing/caching single tables will not fix hop architecture

## Limitations

- Default CI matrix skips wire adapters when builder `resolve()` fails (no Docker in upstream bench).
- AWS baselines above are **debug** builds; do not use for capacity planning until the ceiling campaign runs `--release`.
- Native Mongo `find` / Redis secondary-index pushdown still future; correctness uses in-process filter after load.
- Cross-backend nested `EXISTS` still assumes co-located child SQL tables at the Project engine; harness ensures tables on both backends as a short-term fix.
- 1M prefill opt-in; default query depth sweep stops at 100k unless `--prefill-sweep` extended.

See [EXPERIMENTS.md](EXPERIMENTS.md) for full registry and CLI flags.
