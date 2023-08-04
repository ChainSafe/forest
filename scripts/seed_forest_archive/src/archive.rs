use super::{
    ChainEpoch, ChainEpochDelta, DIFF_STEP, EPOCH_DURATION_SECONDS, EPOCH_STEP,
    MAINNET_GENESIS_TIMESTAMP,
};
use anyhow::Result;
use chrono::NaiveDateTime;
use reqwest::Url;
use std::path::Path;
use std::process::Command;

fn is_available(url: &str) -> Result<bool> {
    let url = Url::parse(url)?;
    let client = reqwest::blocking::Client::new();
    let resp = client.head(url).send()?;
    Ok(resp.status().is_success())
}

pub fn epoch_to_date(epoch: ChainEpoch) -> String {
    NaiveDateTime::from_timestamp_opt(
        (MAINNET_GENESIS_TIMESTAMP + epoch * EPOCH_DURATION_SECONDS) as i64,
        0,
    )
    .unwrap_or_default()
    .format("%Y-%m-%d")
    .to_string()
}

pub fn lite_snapshot_name(epoch: ChainEpoch) -> String {
    let date = epoch_to_date(epoch);
    format!("forest_snapshot_mainnet_{date}_height_{epoch}.forest.car.zst")
}

pub fn diff_snapshot_name(epoch: ChainEpoch, range: ChainEpochDelta) -> String {
    let date = epoch_to_date(epoch);
    format!("forest_diff_mainnet_{date}_height_{epoch}+{range}.forest.car.zst")
}

pub fn upload_lite_snapshot(path: &Path) -> Result<()> {
    let status = Command::new("aws")
        .arg("--endpoint")
        .arg(super::R2_ENDPOINT)
        .arg("s3")
        .arg("cp")
        .arg(path)
        .arg("s3://mainnet/lite/")
        .status()?;
    anyhow::ensure!(status.success(), "failed to upload lite snapshot");
    Ok(())
}

pub fn upload_diff_snapshot(path: &Path) -> Result<()> {
    let status = Command::new("aws")
        .arg("--endpoint")
        .arg(super::R2_ENDPOINT)
        .arg("s3")
        .arg("cp")
        .arg(path)
        .arg("s3://mainnet/diff/")
        .status()?;
    anyhow::ensure!(status.success(), "failed to upload diff snapshot");
    Ok(())
}

pub fn has_lite_snapshot(epoch: ChainEpoch) -> Result<bool> {
    let name = lite_snapshot_name(epoch);
    is_available(&format!(
        "https://forest-archive.chainsafe.dev/mainnet/lite/{name}"
    ))
}

pub fn has_diff_snapshot(epoch: ChainEpoch, range: ChainEpochDelta) -> Result<bool> {
    let name = diff_snapshot_name(epoch, range);
    is_available(&format!(
        "https://forest-archive.chainsafe.dev/mainnet/diff/{name}"
    ))
}

pub fn has_complete_round(round: u64) -> Result<bool> {
    if !has_lite_snapshot(round * EPOCH_STEP)? {
        return Ok(false);
    }
    for n in 0..EPOCH_STEP / DIFF_STEP {
        if !has_diff_snapshot(round * EPOCH_STEP, n * DIFF_STEP)? {
            return Ok(false);
        }
    }
    Ok(true)
}
