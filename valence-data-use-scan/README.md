# uf-valence-data-use-scan

Build-time catalog of Valence `*_used` + `use_!` call sites for the valence-uf-app
Data uses UI.

## Features

- **Workspace scan** — Discovers `*_used` calls across Cargo workspace members so
  host SSR can ship a static catalog. Call `generate` once from `build.rs`.
  See crate rustdoc [Getting started](https://docs.rs/uf-valence-data-use-scan).
- **Purpose extraction** — Reads nearby `use_!(…)` markdown for the catalog.
- **Target classification** — Maps receivers to Schema / Trait / Unscoped for the
  ops UI surfaces.
- **Test exclusion** — Optional omit of `tests/` paths from the UI snapshot via
  `Config::exclude_tests_from_snapshot`.

## Getting started

`uf-valence-data-use-scan` turns declared `*_used` / `use_!` call sites into a
static `DATA_USES` slice. Call `generate` from a host `build.rs` after adding this
crate as a `build-dependency`, once per compile.

```rust,no_run
use std::path::PathBuf;
use valence_data_use_scan::{generate, Config};

fn main() -> Result<(), valence_data_use_scan::DataUseScanError> {
    let workspace_root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .expect("workspace root")
        .to_path_buf();
    generate(&Config {
        workspace_root,
        out_dir: PathBuf::from(std::env::var("OUT_DIR").unwrap()),
        exclude_tests_from_snapshot: true,
    })
}
```

Then in SSR code:

```rust,ignore
include!(concat!(env!("OUT_DIR"), "/data_uses.rs"));
println!("data-use catalog rows: {}", DATA_USES.len());
```

See the crate rustdoc for `generate`, target classification, and test exclusion.

## Perf follow-up (TM-PERF-1)

Measured locally (2026-09-12):

| Scenario | Result |
|----------|--------|
| Before host scan | valence-app `build.rs` had no data-use scan |
| After fixture scan (`uf-valence-data-use-scan` tests) | ~30 ms wall for small fixture workspaces |
| Valence workspace path walk (445 `.rs` files) | ~25 ms walk only |

Catalog-bin / committed snapshot is not required at this cost. Re-measure on L5 marketing/host builds after enabling monorepo-wide `workspace_root` scans.
