// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fs;

use anyhow::{ensure, Result};
use assert_cmd::Command;
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
    let filenames = [
        "forest_snapshot_calibnet_2022-11-22_height_1.car",
        "forest_snapshot_calibnet_2022-11-22_height_2.car",
    ];
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
fn test_snapshot_subcommand_list_invalid_dir() -> Result<()> {
    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("list")
        .arg("--snapshot-dir")
        .arg("/this/is/dummy/path")
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.trim_end();
    ensure!(output.ends_with("No local snapshots"));

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
        .arg("--force")
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
        .arg("--force")
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

#[test]
fn test_snapshot_subcommand_prune_empty() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("prune")
        .arg("--force")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(output.contains("No files to delete"), output);

    Ok(())
}

#[test]
fn test_snapshot_subcommand_prune_single() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = ["forest_snapshot_calibnet_2022-09-28_height_1342143.car"];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;

    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("prune")
        .arg("--force")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(output.contains("No files to delete"), output);

    Ok(())
}

#[test]
fn test_snapshot_subcommand_prune_single_with_custom() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = [
        "forest_snapshot_calibnet_2022-09-28_height_1342143.car",
        "custom.car",
    ];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;

    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("prune")
        .arg("--force")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(output.contains("No files to delete"), output);

    Ok(())
}

#[test]
fn test_snapshot_subcommand_prune_double() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = [
        "forest_snapshot_calibnet_2022-10-10_height_1376736.car",
        "forest_snapshot_calibnet_2022-09-28_height_1342143.car",
        "custom.car",
    ];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;

    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("prune")
        .arg("--force")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(
        output.contains("forest_snapshot_calibnet_2022-09-28_height_1342143.car"),
        output
    );
    ensure!(
        output.contains("forest_snapshot_calibnet_2022-09-28_height_1342143.sha256sum"),
        output
    );

    Ok(())
}

#[test]
fn test_snapshot_subcommand_clean_empty() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("clean")
        .arg("--force")
        .arg("--snapshot-dir")
        .arg(tmp_dir.path().as_os_str().to_str().unwrap_or_default())
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(output.contains("No files to delete"), output);

    Ok(())
}

#[test]
fn test_snapshot_subcommand_clean_snapshot_dir_not_accessible() -> Result<()> {
    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("clean")
        .arg("--force")
        .arg("--snapshot-dir")
        .arg("/turbo-cthulhu")
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout)?.to_owned();
    ensure!(
        output.contains("Target directory not accessible. Skipping."),
        output
    );

    Ok(())
}

#[test]
fn test_snapshot_subcommand_clean_one() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = ["forest_snapshot_calibnet_2022-10-10_height_1376736.car"];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;
    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("clean")
        .arg("--force")
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
fn test_snapshot_subcommand_clean_more() -> Result<()> {
    let tmp_dir = TempDir::new().unwrap();
    let filenames = [
        "forest_snapshot_calibnet_2022-10-10_height_1376736.car",
        "forest_snapshot_calibnet_2022-09-28_height_1342143.car",
        "custom.car",
    ];
    setup_data_dir(&tmp_dir, filenames.as_slice())?;
    let cmd = cli()?
        .arg("--chain")
        .arg("calibnet")
        .arg("snapshot")
        .arg("clean")
        .arg("--force")
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
