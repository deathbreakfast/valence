#![allow(clippy::expect_used, clippy::unwrap_used)] // build.rs: fail fast on codegen errors

//! Product-shaped host — schema scan via valence-codegen defaults.

fn main() {
    valence_codegen::build().expect("valence codegen failed");
}
