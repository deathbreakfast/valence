# codegen-host

End-to-end proof: host-owned `schemas/` + `valence_codegen::build()` →
`valence::include_generated_models!()` → generated `impl Model` against the `valence` crate.
Walk through create/get/merge on the generated `Widget` in `src/lib.rs` (`#[cfg(test)]`).

## Run

```bash
cargo test -p codegen-host
```

See also [`valence-codegen/README.md`](../../valence-codegen/README.md) and the public crate rustdoc Getting started §3–4.
