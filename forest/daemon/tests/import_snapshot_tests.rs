// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod tests;

use anyhow::Result;

use crate::tests::{cli, create_tmp_config};

//#[cfg(feature = "slow_tests")]
#[test]
fn importing_bad_snapshot_should_fail() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;
    let temp_file = data_dir.path().join("bad-snapshot.car");
    std::fs::write(&temp_file, "bad-snpashot")?;
    cli()?
        .arg("--rpc-address")
        .arg("127.0.0.0:0")
        .arg("--metrics-address")
        .arg("127.0.0.0:0")
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .arg("--import-snapshot")
        .arg(temp_file)
        .assert()
        .failure();

    Ok(())
}
