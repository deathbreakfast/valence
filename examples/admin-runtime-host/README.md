# admin-runtime-host

Standalone smoke for wiring admin-runtime surfaces against a mem backend: global `SchemaRegistry` and `TraitRegistry` listing, plus `QueryCore` point reads and `latest_ids`. No UI crate — this is the integrator reference for query/admin tooling that sits beside generated models.

## Prerequisites

- Any prior host that boots `Valence` (`quickstart` or workspace [`codegen-host`](../codegen-host/)).
- Optional: schemas linked into the binary so registry lists are non-empty.

## What to look at (file order)

1. [`src/main.rs`](src/main.rs) — seed row, build `Valence`, registry dumps, `QueryCore` reads (Photon-style steps).

## Run / verify

```bash
cargo run -p admin-runtime-host
```

**Success signal:** stdout prints schema/trait name lists, seeded entity JSON, `latest_ids`, and `admin-runtime-host: OK`.

```bash
cargo check -p admin-runtime-host
```

## Next step

Surreal embedded inventory bootstrap: [`embedded-bootstrap`](../embedded-bootstrap/) — or custom engine checklist: [`acme-valence-backend-stub`](../acme-valence-backend-stub/).
