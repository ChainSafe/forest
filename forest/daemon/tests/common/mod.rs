// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use anyhow::Result;
use assert_cmd::Command;
use tempfile::TempDir;

pub fn cli() -> Result<Command> {
    Ok(Command::cargo_bin("forest")?)
}

pub trait CommonArgs {
    fn common_args(&mut self) -> &mut Self;
}

impl CommonArgs for Command {
    fn common_args(&mut self) -> &mut Self {
        self.arg("--rpc-address")
            .arg("127.0.0.1:0")
            .arg("--metrics-address")
            .arg("127.0.0.1:0")
            .arg("--exit-after-init")
    }
}

pub trait CommonEnv {
    fn common_env(&mut self) -> &mut Self;
}

impl CommonEnv for Command {
    // Always downloads proofs to same location to lower the overall test time
    // (by reducing multiple "fetching param file" steps).
    fn common_env(&mut self) -> &mut Self {
        self.env("FIL_PROOFS_PARAMETER_CACHE", "/tmp/forest-test-fil-proofs")
    }
}

pub fn create_tmp_config() -> Result<(PathBuf, TempDir)> {
    let temp_dir = tempfile::tempdir()?;

    let config = format!(
        r#"
[client]
data_dir = "{}"

[chain.network]
type = "calibnet"
"#,
        temp_dir.path().display()
    );

    let config_file = temp_dir.path().join("config.toml");
    std::fs::write(&config_file, config)?;

    Ok((config_file, temp_dir))
}
