// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use crate::common::{create_tmp_config, daemon, CommonArgs, CommonEnv};

// Ignored because it's flaky.
#[test]
#[ignore]
fn importing_bad_snapshot_should_fail() {
    let (config_file, data_dir) = create_tmp_config();
    let temp_file = data_dir.path().join("bad-snapshot.car");
    std::fs::write(&temp_file, "bad-snapshot").unwrap();
    daemon()
        .common_env()
        .chain("calibnet")
        .arg("--rpc-address")
        .arg("127.0.0.1:0")
        .arg("--metrics-address")
        .arg("127.0.0.1:0")
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .arg("--import-snapshot")
        .arg(temp_file)
        .arg("--halt-after-import")
        .assert()
        .failure();
}
