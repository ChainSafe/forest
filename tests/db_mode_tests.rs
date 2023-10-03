// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use crate::common::{create_tmp_config, daemon, CommonArgs, CommonEnv};

#[test]
fn current_mode_should_create_current_version_if_no_migrations() -> anyhow::Result<()> {
    let (config_file, data_dir) = create_tmp_config();

    daemon()
        .common_env()
        // In its absence, the default will be "current" anyway, but let's make it explicit.
        .env("FOREST_DB_DEV_MODE", "current")
        .common_args()
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .assert()
        .success();

    let forest_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    assert!(data_dir
        .path()
        .join("calibnet")
        .join(forest_version)
        .exists());

    Ok(())
}

#[test]
fn development_mode_should_create_named_db() -> anyhow::Result<()> {
    let (config_file, data_dir) = create_tmp_config();

    daemon()
        .common_env()
        .env("FOREST_DB_DEV_MODE", "azathoth")
        .common_args()
        .arg("--config")
        .arg(&config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .assert()
        .success();

    assert!(data_dir.path().join("calibnet").join("azathoth").exists());

    // write something to the created directory to ensure it's not deleted on a re-run
    std::fs::write(
        data_dir
            .path()
            .join("calibnet")
            .join("azathoth")
            .join("chant"),
        "Rlyeh wgah nagl fhtagn",
    )?;

    daemon()
        .common_env()
        .env("FOREST_DB_DEV_MODE", "azathoth")
        .common_args()
        .arg("--config")
        .arg(config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(
            data_dir
                .path()
                .join("calibnet")
                .join("azathoth")
                .join("chant")
        )?,
        "Rlyeh wgah nagl fhtagn"
    );

    Ok(())
}
