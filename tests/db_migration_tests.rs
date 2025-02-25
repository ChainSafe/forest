// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use crate::common::{CommonArgs, CommonEnv, create_tmp_config, daemon};

#[test]
fn future_db_should_not_fail_daemon() {
    let (config_file, data_dir) = create_tmp_config();

    // Create a future, versioned database in the data directory.
    // This should be ignored by the daemon.
    // In the end, we should have two databases in the data directory:
    // - The future database which should not be deleted,
    // - The new, fresh database which should be used by the daemon.

    let bad_db_path = data_dir.path().join("calibnet").join("666.42.13");
    std::fs::create_dir_all(&bad_db_path).unwrap();
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
    assert!(
        data_dir
            .path()
            .join("calibnet")
            .join(forest_version)
            .exists()
    );
}
