// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use anyhow::Result;

use crate::common::{create_tmp_config, daemon, CommonEnv};

#[test]
fn importing_bad_snapshot_should_fail() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;
    let temp_file = data_dir.path().join("bad-snapshot.car");
    std::fs::write(&temp_file, "bad-snapshot")?;
    daemon()?
        .common_env()
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

    Ok(())
}
