use std::process::Command;
use std::path::{Path, PathBuf};
use super::{ChainEpoch, ChainEpochDelta};
use super::archive::{lite_snapshot_name,diff_snapshot_name};
use anyhow::Result;

pub fn export(epoch: ChainEpoch, files: Vec<&Path>) -> Result<PathBuf> {
    let output_path = lite_snapshot_name(epoch);
    let status = Command::new("forest-cli")
            .arg("archive")
            .arg("export")
            .arg("--epoch")
            .arg(epoch.to_string())
            .arg("--output-path")
            .arg(&output_path)
            .args(files)
            .status()?;
    anyhow::ensure!(status.success());
    Ok(PathBuf::from(output_path))
}

pub fn export_diff(epoch: ChainEpoch, range: ChainEpochDelta, files: Vec<&Path>) -> Result<PathBuf> {
    let output_path = diff_snapshot_name(epoch, range);
    let status = Command::new("forest-cli")
            .arg("archive")
            .arg("export")
            .arg("--epoch")
            .arg((epoch+range).to_string())
            .arg("--diff")
            .arg(epoch.to_string())
            .arg("--output-path")
            .arg(&output_path)
            .args(files)
            .status()?;
    anyhow::ensure!(status.success());
    Ok(PathBuf::from(output_path))
}

pub fn compress(input: &Path, output: &Path) -> Result<()> {
    let status = Command::new("forest-cli")
            .arg("snapshot")
            .arg("compress")
            .arg("--force")
            .arg("--output")
            .arg(output)
            .arg(input)
            .status()?;
    anyhow::ensure!(status.success());
    Ok(())
}
