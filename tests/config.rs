// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::io::Write;

use forest_filecoin::{Client, Config};
use tempfile::TempDir;

pub mod common;
use crate::common::{cli, daemon, tool, CommonArgs};

#[test]
fn test_config_subcommand_produces_valid_toml_configuration_dump() {
    let cmd = cli()
        .arg("--token")
        .arg("Azazello")
        .arg("config")
        .arg("dump")
        .assert()
        .success();

    let output = &cmd.get_output().stdout;
    toml::from_str::<Config>(std::str::from_utf8(output).unwrap()).expect("Invalid configuration!");
}

#[test]
fn test_download_location_of_proof_parameter_files_env() {
    let tmp_dir = TempDir::new().unwrap();

    tool()
        .env("FIL_PROOFS_PARAMETER_CACHE", tmp_dir.path())
        .arg("fetch-params")
        .arg("--keys")
        .arg("--dry-run")
        .assert()
        .stdout(tmp_dir.into_path().to_string_lossy().into_owned() + "\n")
        .success();
}

#[test]
fn test_download_location_of_proof_parameter_files_default() {
    let tmp_dir = TempDir::new().unwrap();
    let tmp_param_dir = tmp_dir.path().join("filecoin-proof-parameters");
    let config = Config {
        client: Client {
            data_dir: tmp_dir.path().to_path_buf(),
            ..Client::default()
        },
        ..Config::default()
    };

    let mut config_file = tempfile::Builder::new().tempfile().unwrap();
    config_file
        .write_all(toml::to_string(&config).unwrap().as_bytes())
        .expect("Failed writing configuration!");

    tool()
        .env("FOREST_CONFIG_PATH", config_file.path())
        .arg("fetch-params")
        .arg("--keys")
        .arg("--dry-run")
        .assert()
        .stdout(tmp_param_dir.to_string_lossy().into_owned() + "\n")
        .success();
}

// Verify that a configuration path can be set with `--config` flag. We
// assume 'data_dir' will be created iff the configuration is correctly parsed.
#[test]
fn test_config_parameter() {
    let tmp_dir = TempDir::new().unwrap().into_path();
    let config = Config {
        client: Client {
            data_dir: tmp_dir.clone(),
            encrypt_keystore: false,
            ..Client::default()
        },
        ..Config::default()
    };

    std::fs::remove_dir(&tmp_dir).unwrap();

    let mut config_file = tempfile::Builder::new().tempfile().unwrap();
    config_file
        .write_all(toml::to_string(&config).unwrap().as_bytes())
        .expect("Failed writing configuration!");

    daemon()
        .common_args()
        .arg("--config")
        .arg(config_file.path())
        .assert()
        .success();
    assert!(tmp_dir.is_dir());
}

// Verify that a configuration path can be set with FOREST_CONFIG_PATH. We
// assume 'data_dir' will be created iff the configuration is correctly parsed.
#[test]
fn test_config_env_var() {
    let tmp_dir = TempDir::new().unwrap().into_path();
    let config = Config {
        client: Client {
            data_dir: tmp_dir.clone(),
            encrypt_keystore: false,
            ..Client::default()
        },
        ..Config::default()
    };

    std::fs::remove_dir(&tmp_dir).unwrap();

    let mut config_file = tempfile::Builder::new().tempfile().unwrap();
    config_file
        .write_all(toml::to_string(&config).unwrap().as_bytes())
        .expect("Failed writing configuration!");

    daemon()
        .common_args()
        .env("FOREST_CONFIG_PATH", config_file.path())
        .assert()
        .success();
    assert!(tmp_dir.is_dir());
}
