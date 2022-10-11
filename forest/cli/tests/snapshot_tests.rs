// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{ensure, Result};
use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_snapshot_subcommand_dir() -> Result<()> {
    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("dir")
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?
        // Normalize path for windows
        .replace('\\', "/");
    ensure!(output.contains("/snapshots/calibnet"), output);

    Ok(())
}

#[test]
fn test_snapshot_subcommand_list() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = ["snapshot1.car", "snapshot2.car"];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;

    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("list")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    for filename in filenames {
        ensure!(output.contains(filename), output);
    }

    Ok(())
}

#[test]
fn test_snapshot_subcommand_remove_invalid() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = ["snapshot1.car", "snapshot2.car"];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;

    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("remove")
        .arg("dummy.car")
        .arg("--yes")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(output.contains("is not a valid snapshot file path"), output);

    Ok(())
}

#[test]
fn test_snapshot_subcommand_remove_success() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = ["snapshot1.car", "snapshot2.car"];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;

    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("remove")
        .arg(filenames[0])
        .arg("--yes")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(output.contains(filenames[0]), output);
    ensure!(
        output.contains(&filenames[0].replace(".car", ".sha256sum")),
        output
    );

    Ok(())
}

fn cli() -> Result<Command> {
    Ok(Command::cargo_bin("forest-cli")?)
}

fn setup_data_dir(tmp_dir: &TempDir, filenames: &[&str]) -> Result<()> {
    for filename in filenames {
        let mut path = tmp_dir.path().to_path_buf();
        path.push(filename);
        fs::write(&path, "dummy")?;
        path.set_extension("sha256sum");
        fs::write(&path, "dummy")?;
    }
    Ok(())
}
