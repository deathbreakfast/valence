//! A second install is rejected and the first catalog keeps serving (own test binary).

#![allow(clippy::unwrap_used)]

use valence_core::data_use::{CatalogInstallError, DataOp, DataUse, DataUseCatalog, DataUseTarget};

fn rows(n: u32) -> Vec<DataUse> {
    (1..=n)
        .map(|line| DataUse {
            purpose: format!("purpose {line}"),
            file: "server/src/lib.rs".into(),
            line,
            crate_name: "server".into(),
            repository: String::new(),
            target: DataUseTarget::Unscoped,
            op: DataOp::Read,
            method: "execute".into(),
            connection: None,
            referenced_schema: None,
            via_trait: None,
        })
        .collect()
}

#[test]
fn second_install_is_already_installed_and_keeps_first() {
    DataUseCatalog::from_entries(rows(3)).install().unwrap();

    let err = DataUseCatalog::from_entries(rows(5)).install().unwrap_err();
    assert_eq!(
        err,
        CatalogInstallError::AlreadyInstalled {
            existing_rows: 3,
            rejected_rows: 5,
        }
    );
    assert!(err.to_string().contains("already installed (3 rows)"));

    let global = DataUseCatalog::global().unwrap();
    assert_eq!(global.len(), 3, "first catalog must stay installed");
}
