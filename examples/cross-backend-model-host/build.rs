#![allow(clippy::expect_used, clippy::unwrap_used)] // build.rs: fail fast on codegen errors

//! Build-time codegen for cross-backend hop scenarios.

fn main() {
    valence_codegen::build().expect("valence codegen failed");
}
