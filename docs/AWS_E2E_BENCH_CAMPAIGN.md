# AWS E2E & bench campaign runbook

Full matrix correctness and performance campaigns run on AWS — not the constrained local WSL gate.

## Env contract

| Variable | Purpose |
|----------|---------|
| `DATABASE_URL` | Postgres wire |
| `VALENCE_MONGODB_URI` | MongoDB wire |
| `VALENCE_REDIS_URL` | Single Redis |
| `VALENCE_REDIS_URLS` | Redis fleet (comma-separated) for bm-v7 |
| `VALENCE_BENCH_ROCKSDB` | Set `1` to enable surreal-rocksdb rows |
| `VALENCE_BENCH_HARDWARE` | Report tag (e.g. `c6i.xlarge`) |
| `CARGO_BUILD_JOBS` | Prefer `1` on small instances |
| `CARGO_TARGET_DIR` | `target-valence-e2e` or `target-valence-bench` |

## Infra sketch

One campaign host (or small fleet) with:

- Docker/services: Postgres, MongoDB, Redis
- Local engines on-box: mem, sqlite, surreal-mem, surreal-rocksdb, indradb
- Optional Redis fleet for bc multibench

## Commands

Entry script: [`scripts/aws-e2e-bench.sh`](../scripts/aws-e2e-bench.sh).

```bash
# Dry-run (list expected tests/experiments)
./scripts/aws-e2e-bench.sh --dry-run

# Full E2E (all features + hop Cartesian when URLs present)
./scripts/aws-e2e-bench.sh --e2e

# Bench slices
./scripts/aws-e2e-bench.sh --bench read-hammer
./scripts/aws-e2e-bench.sh --bench hop-pairs
./scripts/aws-e2e-bench.sh --bench hybrid-compare   # bm-v26: hybrid vs postgres vs indradb
./scripts/aws-e2e-bench.sh --bench privacy-filter   # bm-v27: query privacy post-filter vs dual-key bypass
./scripts/aws-e2e-bench.sh --bench all
```

New catalog sad paths (`privacy-field-system-only-hidden`, `query-limit-clamped`,
`privacy-bypass-requires-force`) ride the default embedded E2E matrix. URL redaction and
SECURITY wiring are correctness-only (no dedicated bench ID).


For published comparative numbers set `VALENCE_BENCH_RELEASE=1` (enabled by `deploy-and-run.sh hybrid-compare`).

Reports land under `profiling/valence-bench/reports/` with `bench_topology: aws` and `VALENCE_BENCH_HARDWARE`.

## Capacity ceiling campaign (RQ-VW2 / RQ-VW3)

After comparative matrices are green, run a **release** ceiling campaign on the same hardware tag to answer how many clients one database can handle:

| Question | Command shape |
|----------|---------------|
| Concurrency knee | `valence-bench run --experiment bm-v5 --storage <adapter> --concurrency <N> --duration-secs 30` for N in 1,2,4,8,16,32,64,128 |
| Multi-client aggregate | bm-v7 with rising client count + `scripts/bench-aggregate-bc.sh` |
| Optional read ceiling | bm-v20 / bm-v21 with rising concurrency |

Stop a sweep when `error_rate ≥ 0.1%` or p95 multiplies beyond a set factor. Prefer one DB service per ceiling run (avoid co-tenant noise). Record `VALENCE_BENCH_HARDWARE` and `--release` in every report. Details and escalation triggers (indexes / cache) live in [`valence-bench/PERFORMANCE_STUDY.md`](../valence-bench/PERFORMANCE_STUDY.md).

## CI

Self-hosted / AWS runner job runs this script. Local PR gate remains `./scripts/gate.sh` (vocabulary + unit/clippy scope) — not full Cartesian.
