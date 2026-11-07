//! Valence end-to-end tests: matrix, admin and model runtime catalogs, and cross-backend hops.
//!
//! Each file under this directory is a module of one test binary, so the
//! crate's integration tests link once instead of once per file.

mod admin_runtime_catalog;
mod cross_backend_hops;
mod matrix_catalog;
mod model_runtime_catalog;
#[cfg(feature = "surreal-inventory")]
mod support;
