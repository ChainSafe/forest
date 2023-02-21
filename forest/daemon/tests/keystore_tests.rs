// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use anyhow::Result;
use assert_cmd::Command;
use forest_key_management::{ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV, KEYSTORE_NAME};
use tempfile::TempDir;

// https://github.com/ChainSafe/forest/issues/2499
#[test]
fn forest_headless_encrypt_keystore_no_passphrase_should_fail() -> Result<()> {
    let (config_file, _data_dir) = create_tmp_config()?;
    cli()?
        .arg("--config")
        .arg(config_file)
        .arg("--exit-after-init")
        .assert()
        .failure();

    Ok(())
}

#[test]
fn forest_headless_no_encrypt_no_passphrase_should_succeed() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;
    cli()?
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .arg("--exit-after-init")
        .assert()
        .success();

    assert!(data_dir.path().join(KEYSTORE_NAME).exists());

    Ok(())
}

#[test]
fn forest_headless_encrypt_keystore_with_passphrase_should_succeed() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;
    cli()?
        .env(FOREST_KEYSTORE_PHRASE_ENV, "yuggoth")
        .arg("--config")
        .arg(config_file)
        .arg("--exit-after-init")
        .assert()
        .success();

    assert!(data_dir.path().join(ENCRYPTED_KEYSTORE_NAME).exists());

    Ok(())
}

fn cli() -> Result<Command> {
    Ok(Command::cargo_bin("forest")?)
}

fn create_tmp_config() -> Result<(PathBuf, TempDir)> {
    let temp_dir = tempfile::tempdir()?;

    let config = format!(
        r#"
[client]
data_dir = "{}"

[chain]
name = "calibnet"
"#,
        temp_dir.path().display()
    );

    let config_file = temp_dir.path().join("config.toml");
    std::fs::write(&config_file, config)?;

    Ok((config_file, temp_dir))
}
