# valence-core

Ports: `DatabaseBackend`, `DatabaseRouter`, `ValenceBuilder`, host injectable traits.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **Application developers** | Use via the `valence` crate; rarely depend on this crate directly |
| **Host integrators** | Assemble `Valence::builder()` and inject ports |
| **Adapter authors** | Implement `DatabaseBackend` and optional evaluators |

## API documentation

**Source of truth:** `cargo doc -p uf-valence-core --open`

Architecture and port contracts live in rustdoc module pages (including `# Examples` on
`ValenceBuilder`, `DatabaseBackend`, `DatabaseRouter`, and `Model`). Integrator configuration
precedence and env vars are documented in [`../valence/README.md`](../valence/README.md).

**Documentation policy:** item-level docs on core wiring APIs are required for new changes;
full-crate `#![deny(missing_docs)]` is planned incrementally (workspace `missing_docs = allow`).
