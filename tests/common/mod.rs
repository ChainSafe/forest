// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use assert_cmd::{Command, cargo::cargo_bin_cmd};
use tempfile::TempDir;

pub fn cli() -> Command {
    cargo_bin_cmd!("forest-cli")
}

pub fn tool() -> Command {
    cargo_bin_cmd!("forest-tool")
}

pub fn daemon() -> Command {
    cargo_bin_cmd!("forest")
}

pub trait CommonArgs {
    fn common_args(&mut self) -> &mut Self;
}

impl CommonArgs for Command {
    fn common_args(&mut self) -> &mut Self {
        self.arg("--rpc")
            .arg("false")
            .arg("--no-metrics")
            .arg("--no-healthcheck")
            .arg("--exit-after-init")
            .arg("--skip-load-actors")
    }
}

pub trait CommonEnv {
    fn common_env(&mut self) -> &mut Self;
}

impl CommonEnv for Command {
    // Always downloads proofs to same location to lower the overall test time
    // (by reducing multiple "fetching param file" steps).
    fn common_env(&mut self) -> &mut Self {
        match std::env::var("FIL_PROOFS_PARAMETER_CACHE").ok() {
            Some(v) if !v.is_empty() => self,
            _ => self.env("FIL_PROOFS_PARAMETER_CACHE", "/tmp/forest-test-fil-proofs"),
        }
    }
}

pub fn create_tmp_config() -> (PathBuf, TempDir) {
    let temp_dir = tempfile::tempdir().expect("couldn't create temp dir");

    let config = format!(
        r#"
[client]
data_dir = "{}"

[chain]
type = "calibnet"
"#,
        temp_dir.path().display()
    );

    let config_file = temp_dir.path().join("config.toml");
    std::fs::write(&config_file, config).expect("couldn't write config");

    (config_file, temp_dir)
}
