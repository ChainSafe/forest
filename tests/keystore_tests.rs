// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use anyhow::Result;
use forest_auth::{verify_token, JWT_IDENTIFIER};
use forest_key_management::{
    KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV, KEYSTORE_NAME,
};

use crate::common::{cli, create_tmp_config, CommonArgs};

// https://github.com/ChainSafe/forest/issues/2499
#[test]
fn forest_headless_encrypt_keystore_no_passphrase_should_fail() -> Result<()> {
    let (config_file, _data_dir) = create_tmp_config()?;
    cli()?
        .common_args()
        .arg("--config")
        .arg(config_file)
        .assert()
        .failure();

    Ok(())
}

#[test]
fn forest_headless_no_encrypt_no_passphrase_should_succeed() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;
    cli()?
        .common_args()
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .assert()
        .success();

    assert!(data_dir.path().join(KEYSTORE_NAME).exists());

    Ok(())
}

#[test]
fn forest_headless_encrypt_keystore_with_passphrase_should_succeed() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;
    cli()?
        .env(FOREST_KEYSTORE_PHRASE_ENV, "hunter2")
        .common_args()
        .arg("--config")
        .arg(config_file)
        .assert()
        .success();

    assert!(data_dir.path().join(ENCRYPTED_KEYSTORE_NAME).exists());
    Ok(())
}

#[test]
fn should_create_jwt_admin_token() -> Result<()> {
    let (config_file, data_dir) = create_tmp_config()?;
    let token_path = data_dir.path().join("admin-token");
    cli()?
        .common_args()
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .arg("--save-token")
        .arg(&token_path)
        .assert()
        .success();

    // Grab the keystore and the private key
    let keystore = KeyStore::new(KeyStoreConfig::Persistent(data_dir.path().to_owned()))?;
    let key_info = keystore.get(JWT_IDENTIFIER)?;
    let key = key_info.private_key();

    // Validate the token
    assert!(token_path.exists());
    let token = std::fs::read_to_string(token_path)?;
    let allow = verify_token(&token, key)?;
    assert!(allow.contains(&"admin".to_owned()));

    Ok(())
}
