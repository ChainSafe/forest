use super::archive::{diff_snapshot_name, lite_snapshot_name};
use super::{ChainEpoch, ChainEpochDelta};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub fn export(epoch: ChainEpoch, files: Vec<&str>) -> Result<Child> {
    let output_path = lite_snapshot_name(epoch);
    let mut export = Command::new("forest-cli")
        .arg("archive")
        .arg("export")
        .arg("--epoch")
        .arg(epoch.to_string())
        .arg("--output-path")
        .arg("-")
        .args(files)
        .stdout(Stdio::piped())
        .spawn()?;
    Ok(Command::new("aws")
        .arg("--endpoint")
        .arg(super::R2_ENDPOINT)
        .arg("s3")
        .arg("cp")
        .arg("-")
        .arg(format!("s3://forest-archive/mainnet/lite/{output_path}"))
        .stdin(export.stdout.take().unwrap())
        .spawn()?)
}

pub fn export_diff(
    epoch: ChainEpoch,
    range: ChainEpochDelta,
    files: Vec<&Path>,
) -> Result<PathBuf> {
    let output_path = diff_snapshot_name(epoch, range);
    let status = Command::new("forest-cli")
        .arg("archive")
        .arg("export")
        .arg("--epoch")
        .arg((epoch + range).to_string())
        .arg("--depth")
        .arg(range.to_string())
        .arg("--diff")
        .arg(epoch.to_string())
        .arg("--diff-depth")
        .arg("2000")
        .arg("--output-path")
        .arg(&output_path)
        .args(files)
        .status()?;
    anyhow::ensure!(status.success(), "failed to export diff snapshot");
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
    anyhow::ensure!(status.success(), "failed to compress CAR file");
    Ok(())
}
