use super::archive::{diff_snapshot_name, lite_snapshot_name};
use super::{ChainEpoch, ChainEpochDelta};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::spawn;

pub fn export(epoch: ChainEpoch, files: Vec<String>) -> Result<Child> {
    let output_path = lite_snapshot_name(epoch);
    let mut export = Command::new("forest-cli")
        .arg("archive")
        .arg("export")
        .arg("--epoch")
        .arg(epoch.to_string())
        .arg("--depth")
        .arg("900")
        .arg("--output-path")
        .arg("-")
        .args(files)
        .env("RUST_LOG", "error")
        .stdout(Stdio::piped())
        .spawn()?;
    let export_stdout = export.stdout.take().unwrap();
    spawn(|| {
        let output = export.wait_with_output().unwrap();
        if !output.status.success() {
            eprintln!("Failed to export snapshot. Error message:");
            eprintln!("{}", std::str::from_utf8(&output.stderr).unwrap_or_default());
            std::process::exit(1);
        }
    });
    Ok(Command::new("aws")
        .arg("--endpoint")
        .arg(super::R2_ENDPOINT)
        .arg("s3")
        .arg("cp")
        .arg("--content-type")
        .arg("application/zstd")
        .arg("-")
        .arg(format!("s3://forest-archive/mainnet/lite/{output_path}"))
        .stdin(export_stdout)
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
