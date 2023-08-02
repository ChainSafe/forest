use std::process::Command;
use std::path::Path;
use super::ChainEpoch;
use super::archive::lite_snapshot_name;
use anyhow::Result;

pub fn export(epoch: ChainEpoch, files: Vec<&Path>) -> Result<()> {
    let output_path = lite_snapshot_name(epoch);
    let status = Command::new("forest-cli")
            .arg("archive")
            .arg("export")
            .arg("--epoch")
            .arg(epoch.to_string())
            .arg("--output-path")
            .arg(output_path)
            .args(files)
            .status()?;
    anyhow::ensure!(status.success());
    Ok(())
}
