# valence-bench

Binary CLI for synthetic Valence throughput experiments (`bm-v0`..`bm-v31`). Highlights: [PERFORMANCE.md](PERFORMANCE.md). Full experiment registry and AWS baselines come from AWS campaign runs.

## Quick start

```bash
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=target-valence-bench
cargo run -p valence-bench -- experiments
cargo run -p valence-bench -- matrix adapter-minimal --storage mem,sqlite
cargo run -p valence-bench -- run --experiment bm-v5 --storage sqlite --concurrency 32 --duration-secs 10
cargo run -p valence-bench --release -- run --experiment bm-v29 \
  --storage sqlite --duration-secs 30 --concurrency 32 --prefill 10000
VALENCE_BENCH_CLIENT_INDEX=0 cargo run -p valence-bench --release -- run --experiment bm-v30 \
  --storage sqlite --duration-secs 30 --concurrency 32 --prefill 10000 --bench-clients 2
```

JSON reports: `profiling/valence-bench/reports/{experiment}-{matrix}-{hardware}.json`.

## Verify

```bash
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=target-valence-bench
cargo run -p valence-bench -- experiments
cargo run -p valence-bench -- run --experiment bm-v0 --storage mem --telemetry off --topology embedded --ops 1000
cargo test -p valence-bench -- --test-threads=1
```

See [PERFORMANCE.md](PERFORMANCE.md). AWS campaign coverage notes may live under `docs/` when present in-tree; full operator scripts are in the lab repo.
