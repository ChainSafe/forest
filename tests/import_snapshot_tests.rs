// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use crate::common::{CommonEnv, create_tmp_config, daemon};

#[test]
fn importing_bad_snapshot_should_fail() {
    let (config_file, data_dir) = create_tmp_config();
    let temp_file = data_dir.path().join("bad-snapshot.car");
    std::fs::write(&temp_file, "bad-snapshot").unwrap();
    daemon()
        .common_env()
        .arg("--rpc")
        .arg("false")
        .arg("--no-metrics")
        .arg("--no-healthcheck")
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .arg("--skip-load-actors")
        .arg("--import-snapshot")
        .arg(temp_file)
        .assert()
        .failure();
}
