# codegen-host

End-to-end proof: host-owned `schemas/` + `valence_codegen::build()` →
`valence::include_generated_models!()` → generated `impl Model` against the `valence` crate.

## Audience

Application developers learning the codegen pipeline.

## Run

```bash
cargo check -p codegen-host
cargo test -p codegen-host --test compile_generated_models
```

See also [`valence-codegen/README.md`](../../valence-codegen/README.md) and the public crate rustdoc Getting started §3–4.
