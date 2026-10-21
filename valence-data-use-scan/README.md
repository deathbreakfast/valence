# uf-valence-data-use-scan

Build-time catalog of every Valence data use a deployment declares. The host's `build.rs`
generates a `data_use_catalog()` function, the host installs it at boot, and the
valence-uf-app Data uses pages read it.

## Features

- **Deployment scan** — Collects every purpose-required call in the host package and every
  crate it links, with the features the host was compiled with. Call `generate` once from
  the host's `build.rs`.
- **Deployment inventory** — The host lists binaries that run outside the server (Chronon,
  Boson, or Photon runtimes, workers in other repositories) under
  `[target.'cfg(any())'.dependencies]`. Cargo resolves and fetches them without compiling
  them, and the scan reads their sources.
- **Purpose extraction** — Reads the `use_!(…)` markdown next to each call.
- **Target classification** — Maps receivers to Schema / Trait / Unscoped for the ops UI.
- **Test exclusion** — `Config::exclude_tests_from_snapshot` omits test modules under
  `src/` from the catalog.
- **Purpose lint** — `lint_purpose` flags template sentences and hedges in `use_!` text.
  The `valence-purpose-lint` binary counts hits per file (see
  [Checking purposes in a checkout](#checking-purposes-in-a-checkout)).
- **Connection hops** — Classifies forward loads and edge mutates, and optionally bakes the
  peer schema via `Config::connection_edges` for Referenced Reads / Updates.

## Getting started

Add this crate to the host server package's `[build-dependencies]` and call `generate` from
its `build.rs`:

```rust,no_run
use std::path::PathBuf;
use valence_data_use_scan::{generate, Config, HostPackage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let workspace_root = manifest_dir.parent().ok_or("host has no parent")?.to_path_buf();
    generate(&Config {
        workspace_root,
        out_dir: PathBuf::from(std::env::var("OUT_DIR")?),
        host: HostPackage::FromBuildScript,
        exclude_tests_from_snapshot: true,
        connection_edges: vec![],
    })?;
    Ok(())
}
```

Declare out-of-process components in the same package's `Cargo.toml`. Each one needs a
library target; a binary-only crate needs a one-line `src/lib.rs`.

```toml
[target.'cfg(any())'.dependencies]
chronon-runtime = { path = "../chronon-runtime" }
ocr-chronon-worker = { git = "https://github.com/acme/ocr-worker", branch = "main" }
```

Then include the generated file once and install the catalog at boot:

```rust,ignore
mod generated {
    include!(concat!(env!("OUT_DIR"), "/data_uses.rs"));
}

let catalog = generated::data_use_catalog().install()?;
println!("data-use catalog rows: {}", catalog.len());
```

The crate rustdoc covers feature gating, the inventory contract, test exclusion, and
connection hops. `tests/deployment_inventory.rs` is a runnable fixture deployment.

## Perf follow-up (TM-PERF-1)

Measured locally (2026-10-01):

| Scenario | Result |
|----------|--------|
| Fixture deployment scan (7 packages, two `cargo metadata` calls) | ~40 ms per `generate` |
| Valence workspace path walk (445 `.rs` files) | ~25 ms walk only (2026-09-12) |

Most of the cost is `cargo metadata`. Host builds rerun the scan only when `Cargo.lock`,
a manifest, or a local package's `src/` changes. Re-measure on the L5 site host after
wiring.

## Checking purposes in a checkout

This is for contributors rewriting `use_!` purposes, not for host developers. Point the
binary at a checkout to count the purposes that still carry a migration template sentence
(`BAN_TEMPLATE`) or a hedge such as "not necessarily" or "unless that feature" (`HEDGE`):

```sh
cargo run -p uf-valence-data-use-scan --bin valence-purpose-lint -- path/to/checkout
```

```text
src/applications/service.rs: 4
src/images/service.rs: 2
6 template/hedge hits in 2 files; 6 of 41 use_! sites
```

The run exits 0 even when it finds hits, so you can use it as an inventory before any CI
gate. It exits 1 if a file could not be parsed (listed on stderr) and 2 on a usage error or
an unreadable path. `--sites` adds `file:line: CODE phrase` for each hit, and `--csv` prints
every site with all of its gap codes. Only `BAN_TEMPLATE` and `HEDGE` count as hits;
others, such as `TOO_THIN`, appear only in the CSV. Test paths and `target/` are skipped.

To fix a hit, follow the call's result to where it is used, then write what data is read or
written, why, and which product feature and screen it serves, in plain words.
