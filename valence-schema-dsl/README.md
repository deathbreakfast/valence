# valence-schema-dsl

Shared syn-based parser for `valence_schema!` and `valence_trait_schema!`.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **Maintainers** | One grammar/AST shared by macros and codegen |
| **App developers** | Prefer the `valence` crate macros — do not depend on this crate directly |

## Role

- **`valence-macros`** — proc-macro expansion (metadata + `inventory`)
- **`valence-codegen`** — build-time scan of host `schemas/*.rs` → Model CRUD

Both paths call the same parse/lower APIs so DSL syntax and semantics cannot drift.

## Boundaries

Depends only on `syn` / `quote` / `proc-macro2`. Must **not** depend on `valence-core`, the `valence` crate, `valence-macros`, or `valence-codegen`.

## Verify

```bash
cargo test -p uf-valence-schema-dsl
```
