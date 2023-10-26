// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use assert_cmd::Command;
use tempfile::TempDir;

pub fn cli() -> Command {
    Command::cargo_bin("forest-cli").unwrap()
}

pub fn tool() -> Command {
    Command::cargo_bin("forest-tool").unwrap()
}

pub fn daemon() -> Command {
    Command::cargo_bin("forest").unwrap()
}

pub trait CommonArgs {
    fn common_args(&mut self) -> &mut Self;

    fn chain(&mut self, chain: impl AsRef<std::ffi::OsStr>) -> &mut Self;
}

impl CommonArgs for Command {
    fn common_args(&mut self) -> &mut Self {
        self.arg("--rpc-address")
            .arg("127.0.0.1:0")
            .arg("--metrics-address")
            .arg("127.0.0.1:0")
            .arg("--exit-after-init")
            .arg("--skip-load-actors")
    }

    fn chain(&mut self, chain: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.arg("--chain").arg(chain)
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

pub fn create_tmp_config() -> (PathBuf, TempDir) {
    let temp_dir = tempfile::tempdir().expect("couldn't create temp dir");

    let config = format!(
        r#"
[client]
data_dir = "{}"
"#,
        temp_dir.path().display()
    );

    let config_file = temp_dir.path().join("config.toml");
    std::fs::write(&config_file, config).expect("couldn't write config");

    (config_file, temp_dir)
}
