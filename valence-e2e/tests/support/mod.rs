//! E2e support — inventory schema fixture for surreal-inventory scenarios.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
#![allow(dead_code)]

use valence::prelude::*;

pub const E2E_INVENTORY_DB: DatabaseFromEngine =
    Database::from_engine("e2e_inventory", valence::SURREAL_ENGINE_ID);

valence_schema! {
    E2eInventorySmoke {
        table: "e2e_inventory_smoke",
        version: "0.1.0",
        database: E2E_INVENTORY_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ],
    }
}
