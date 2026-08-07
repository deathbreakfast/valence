# valence-bench

Binary CLI for synthetic Valence throughput experiments (`bm-v0`..`bm-v25`). Highlights: [PERFORMANCE.md](PERFORMANCE.md). Full experiment registry and AWS baselines come from AWS campaign runs.

## Quick start

```bash
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=target-valence-bench
cargo run -p valence-bench -- experiments
cargo run -p valence-bench -- matrix adapter-minimal --storage mem,sqlite
cargo run -p valence-bench -- run --experiment bm-v5 --storage sqlite --concurrency 32 --duration-secs 10
```

JSON reports: `profiling/valence-bench/reports/{experiment}-{matrix}-{hardware}.json`.

## Verify

```bash
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=target-valence-bench
cargo run -p valence-bench -- experiments
cargo run -p valence-bench -- run --experiment bm-v0 --storage mem --telemetry off --topology embedded --ops 1000
```

See [PERFORMANCE.md](PERFORMANCE.md). AWS campaign coverage notes may live under `docs/` when present in-tree; full operator scripts are in the lab repo.
