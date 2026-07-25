# Valence bench experiments (bm-v0..bm-v25)

Synthetic throughput experiments driven by `valence-bench` and shared with `valence-testkit` bootstrap/scenarios.

See [PERFORMANCE_STUDY.md](PERFORMANCE_STUDY.md) for research questions and methodology.

## CLI

```bash
valence-bench experiments
valence-bench run --experiment bm-v5 --storage redis --redis-url redis://127.0.0.1:6379 --concurrency 32 --duration-secs 30
valence-bench matrix adapter-minimal --storage mem,sqlite,mongodb --mongodb-uri mongodb://127.0.0.1:27017
```

### Matrix flags

| Flag | Values |
|------|--------|
| `--storage` | `mem`, `sqlite`, `mongodb`, `indradb`, `redis`, `surreal-mem`, `surreal-rocksdb`, `acme-stub`, `postgres` |
| `--telemetry` | `off`, `console`, `recording` |
| `--topology` | `embedded` (default); `remote-stub` rejected |
| `--redis-url` | Redis URL (overrides env) |
| `--mongodb-uri` | MongoDB URI (overrides env) |
| `--postgres-url` | Postgres URL (overrides env) |
| `--redis-urls` | Comma-separated Redis fleet URLs |

### Sweep flags

| Flag | Default | Used by |
|------|---------|---------|
| `--prefill` | 10000 | bm-v11..v14 |
| `--prefill-sweep` | — | comma list e.g. `1000,10000,100000` |
| `--duration-secs` | 30 | bm-v5, bm-v6, bm-v7 |
| `--concurrency` | 64 | bm-v5, bm-v7 |
| `--bench-clients` | 1 | bm-v7 |
| `--query-iters` | 1000 | query track |
| `--privacy-sleep-us` | 0 | bm-v17 |
| `--warmup` | 0 | all timed loops |
| `--ops` | per experiment | serial micro-benchs |

Env: `VALENCE_BENCH_HARDWARE` (default `dev-wsl`), `VALENCE_BENCH_CLIENT_INDEX` (bc multibench). Wire adapter env vars are optional fallbacks when CLI flags are omitted — see each `valence-backend-*` crate README / builder rustdoc.

Reports land under `profiling/valence-bench/reports/` as JSON.

## Registry

| ID | Experiment | Storage focus |
|----|------------|---------------|
| **bm-v0** | Serial create+get | all available |
| **bm-v1** | Compiled query fan-out | mem |
| **bm-v2** | Instrumentation overhead | mem + recording |
| **bm-v3** | Surreal CRUD | surreal-* |
| **bm-v4** | Acme stub throughput | acme-stub |
| **bm-v5** | Write firehose (adapter) | all available |
| **bm-v6** | ORM write firehose | all available |
| **bm-v7** | bc multibench firehose | all available |
| **bm-v8** | Merge throughput | all available |
| **bm-v9** | Soft-delete queue | mem, sqlite, surreal |
| **bm-v11** | Compiled query @ prefill | all available |
| **bm-v12** | ORM query @ prefill | all available |
| **bm-v13** | Filter shape compare | all available |
| **bm-v14** | Pagination offset | all available |
| **bm-v15** | Hop pair smoke (delegates to hop harness) | mem→sqlite |
| **bm-v16** | Privacy gate overhead | mem, sqlite |
| **bm-v17** | Privacy sleep sweep | mem |
| **bm-v27** | Query privacy post-filter vs bypass | mem, sqlite (+ wire on AWS) |
| **bm-v18** | Telemetry overhead | mem |
| **bm-v19** | Map/RSS @ 10k rows | mem, sqlite |
| **bm-v20** | Get-by-id hammer (hot + unique; cache on/off) | all available |
| **bm-v21** | Filtered equality query hammer | all available |
| **bm-v22** | Full-scan / large-N ORM list | all available |
| **bm-v23** | Complex query (filter+order+page) | all available |
| **bm-v24** | Cross-backend hop depth-2 | pair matrix |
| **bm-v25** | Nested hop chain depth-3/4 | representative chains |
| **bm-v26** | Hybrid vs postgres vs indradb (get / compiled query / M2M hop) | hybrid, postgres, indradb |

## Matrix slices

| Slice | Experiments | Default storage |
|-------|-------------|-----------------|
| `adapter-minimal` | bm-v0, bm-v11 @ prefill=10k | mem, sqlite |
| `write-sweep` | bm-v5 @ 10s, C=32 | comma `--storage` |
| `query-depth` | bm-v11, bm-v12 @ prefill=10k | comma `--storage` |
| `overhead` | bm-v16, bm-v18 | mem |
| `read-hammer` | bm-v20 | comma `--storage` |
| `query-real` | bm-v21, bm-v22, bm-v23 | comma `--storage` |
| `hop-pairs` | bm-v24 | comma `--storage` |
| `hop-chains` | bm-v25 | comma `--storage` |
| `hybrid-compare` | bm-v26 | hybrid, postgres, indradb |

## AWS campaigns

Full matrix E2E + benches run on AWS — see [docs/AWS_E2E_BENCH_CAMPAIGN.md](../docs/AWS_E2E_BENCH_CAMPAIGN.md) and `scripts/aws-e2e-bench.sh`. Coverage map: [docs/E2E_BENCH_COVERAGE.md](../docs/E2E_BENCH_COVERAGE.md).

```bash
# Run N clients with distinct index:
VALENCE_BENCH_CLIENT_INDEX=0 cargo run -p valence-bench -- run --experiment bm-v7 --storage mem &
VALENCE_BENCH_CLIENT_INDEX=1 cargo run -p valence-bench -- run --experiment bm-v7 --storage mem &
wait
bash scripts/bench-aggregate-bc.sh profiling/valence-bench/reports 'bm-v7-*'
```

## AWS cluster campaigns (deferred)

When quota frees:

- Pass `--redis-urls node1:6379,node2:6379` or set `VALENCE_REDIS_URLS`
- Run bm-v7 with multiple bc clients
- Aggregate with `scripts/bench-aggregate-bc.sh`
- Tag reports `bench_topology: remote`

## Isolated target dir

```bash
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=target-valence-bench
cargo run -p valence-bench -- matrix adapter-minimal --storage mem,sqlite
```

RocksDB rows require `VALENCE_BENCH_ROCKSDB=1` and `surreal-rocksdb` feature at build time.
