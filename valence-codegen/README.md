# valence-codegen

Build-time code generation for Valence models from `valence_schema!` and `valence_trait_schema!` sources. Host crates own schema scan paths in `build.rs` / CI; generators live here while DSL parse/AST is shared with `valence-macros` via `valence-schema-dsl`.

Depends on `valence-core` + shared [`valence-schema-dsl`](../valence-schema-dsl/README.md); generated output targets the `valence` crate.

## Parser / metadata parity

Schema files are parsed with the same syn grammar as `valence_schema!`. Generated metadata honors:

- `database:` evaluator expressions (use `crate::MY_DB` paths that resolve from the include site)
- policy rule evaluators (not name-only stubs)
- `ownership:` and `composite_key:`

Policy emission matches the macro path (`Box::leak` of `&dyn PolicyEvaluator`).

## Host `build.rs`

Default layout: `$CARGO_MANIFEST_DIR/schemas/` scanned recursively for `*_valence_schema.rs` (and optional `*_valence_trait.rs`) → `$OUT_DIR/generated_models.rs`. Nested dirs such as `schemas/valence/` are included.

```rust
fn main() {
    valence_codegen::build().expect("valence codegen failed");
}
```

In the host crate:

```rust
valence::include_generated_models!();
```

Custom roots or suffixes: `build_with` / `CodegenConfig` / `generate_models`.

Or invoke the CLI (avoids a build-dependency on codegen when cross-compiling):

```bash
valence-generate ./schemas $OUT_DIR
# optional overrides:
valence-generate ./schemas $OUT_DIR _valence_schema.rs _valence_trait.rs
```

Empty `schemas/` directories succeed and still write a header-only `generated_models.rs`.

## Example

See [`examples/codegen-host/`](../examples/codegen-host/).

## Verify

```bash
cargo check -p uf-valence-codegen
cargo test -p uf-valence-codegen
cargo test -p codegen-host
```
