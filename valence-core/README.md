# valence-core

Ports: `DatabaseBackend`, `DatabaseRouter`, `ValenceBuilder`, host injectable traits. Prefer the `valence` facade crate for application code; this crate exposes the port contracts for assembling `Valence::builder()`, injecting host ports, and implementing `DatabaseBackend` (and optional evaluators) in adapter crates.

## API documentation

**Source of truth:** `cargo doc -p uf-valence-core --open`

Architecture and port contracts live in rustdoc module pages (including `# Examples` on
`ValenceBuilder`, `DatabaseBackend`, `DatabaseRouter`, and `Model`). Integrator configuration
precedence and env vars are documented in [`../valence/README.md`](../valence/README.md).

**Documentation policy:** item-level docs on core wiring APIs are required for new changes;
full-crate `#![deny(missing_docs)]` is planned incrementally (workspace `missing_docs = allow`).
