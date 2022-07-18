// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use assert_cmd::Command;
use forest::cli::Config;
use rand::Rng;
use std::{io::Write, net::SocketAddr, str::FromStr};

#[test]
fn test_config_subcommand_produces_valid_toml_configuration_dump() {
    let cmd = Command::cargo_bin("forest")
        .unwrap()
        .arg("--rpc")
        .arg("true")
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
fn test_overrides_are_reflected_in_configuration_dump() {
    let mut rng = rand::thread_rng();
    let randomized_metrics_host = format!("127.0.0.1:{}", rng.gen::<u16>());

    let cmd = Command::cargo_bin("forest")
        .unwrap()
        .arg("--rpc")
        .arg("true")
        .arg("--token")
        .arg("Azazello")
        .arg("--metrics-address")
        .arg(&randomized_metrics_host)
        .arg("config")
        .arg("dump")
        .assert()
        .success();

    let output = &cmd.get_output().stdout;
    let config = toml::from_str::<Config>(std::str::from_utf8(output).unwrap())
        .expect("Invalid configuration!");

    assert_eq!(
        config.metrics_address,
        SocketAddr::from_str(&randomized_metrics_host).unwrap()
    );
}

#[test]
fn test_reading_configuration_from_file() {
    let mut rng = rand::thread_rng();

    let expected_config = Config {
        metrics_address: SocketAddr::from_str(&format!("127.0.0.1:{}", rng.gen::<u16>())).unwrap(),
        rpc_token: Some("Azazello".into()),
        genesis_file: Some("cthulhu".into()),
        encrypt_keystore: false,
        ..Config::default()
    };

    let mut config_file = tempfile::Builder::new().tempfile().unwrap();
    config_file
        .write_all(toml::to_string(&expected_config).unwrap().as_bytes())
        .expect("Failed writing configuration!");

    let cmd = Command::cargo_bin("forest")
        .unwrap()
        .arg("--rpc")
        .arg("true")
        .arg("--token")
        .arg("Azazello")
        .arg("--config")
        .arg(config_file.path())
        .arg("config")
        .arg("dump")
        .assert()
        .success();

    let output = &cmd.get_output().stdout;
    let actual_config = toml::from_str::<Config>(std::str::from_utf8(output).unwrap())
        .expect("Invalid configuration!");

    assert!(expected_config == actual_config);
}
