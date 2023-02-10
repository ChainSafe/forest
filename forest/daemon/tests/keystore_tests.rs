// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use anyhow::Result;
use assert_cmd::Command;
use tempfile::TempDir;

// https://github.com/ChainSafe/forest/issues/2499
#[test]
fn forest_headless_encrypt_keystore_no_passphrase_should_fail() -> Result<()> {
    let (config_file, _data_dir) = create_tmp_config()?;
    cli()?.arg("--config").arg(config_file).assert().failure();

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
