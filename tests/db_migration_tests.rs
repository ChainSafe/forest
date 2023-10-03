// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use crate::common::{create_tmp_config, daemon, CommonArgs, CommonEnv};
use anyhow::Result;

#[test]
fn failing_migration_should_not_fail_daemon() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;

    // Create an invalid, versioned database in the data directory.
    // This will trigger a migration, which should fail. Forest should be able to recover from
    // this.
    // In the end, we should have two databases in the data directory:
    // - The invalid database which should not be deleted,
    // - The new, fresh database which should be used by the daemon.

    let bad_db_path = data_dir.path().join("calibnet").join("0.12.1");
    std::fs::create_dir_all(&bad_db_path)?;
    daemon()
        .common_env()
        .common_args()
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .assert()
        .success();

    assert!(bad_db_path.exists());

    let forest_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    assert!(data_dir
        .path()
        .join("calibnet")
        .join(forest_version)
        .exists());

    Ok(())
}
