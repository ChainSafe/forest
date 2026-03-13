// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashSet;
use anyhow::{Context as _, bail};
use clap::Args;
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::SystemTime;
use url::Url;

use crate::networks::{NetworkChain, calculate_expected_epoch};
use crate::shim::clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use crate::utils::net::global_http_client;

const LIST_BASE: &str = "https://forest-archive.chainsafe.dev/list";

const LITE_INTERVAL: ChainEpoch = 30_000;
const DIFF_INTERVAL: ChainEpoch = 3_000;

/// Well-known genesis timestamps (Unix seconds).
const MAINNET_GENESIS_TIMESTAMP: u64 = 1598306400; // 2020-08-24T22:00:00Z
const CALIBNET_GENESIS_TIMESTAMP: u64 = 1667326380; // 2022-11-01T14:13:00Z

#[derive(Debug, Args)]
pub struct ArchiveMissingCommand {
    /// Filecoin network chain (e.g., calibnet, mainnet)
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Start epoch (inclusive). Defaults to genesis (epoch 0).
    /// Rounded down to the nearest lite boundary.
    #[arg(long)]
    from: Option<ChainEpoch>,
    /// End epoch (inclusive). Defaults to the current expected epoch minus 3000.
    /// Rounded up to the next diff boundary.
    #[arg(long)]
    to: Option<ChainEpoch>,
}

#[derive(Debug, Deserialize)]
struct ListingItem {
    url: Url,
}

#[derive(Debug, Deserialize)]
struct ListingResponse {
    items: Vec<ListingItem>,
}

/// Extract height from an archive URL.
/// Lite: `..._height_30000.forest.car.zst` → 30000
/// Diff: `..._height_0+3000.forest.car.zst` → 0
fn extract_height(url: &Url) -> Option<ChainEpoch> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_height_(\d+)").unwrap());
    let path = url.path();
    let caps = RE.captures(path)?;
    caps[1].parse().ok()
}

/// Parse a JSON listing response into a set of available heights.
fn parse_listing_heights(data: &ListingResponse) -> HashSet<ChainEpoch> {
    data.items
        .iter()
        .filter_map(|item| extract_height(&item.url))
        .collect()
}

/// Compute the required lite snapshot epochs for a given range.
fn compute_required_lite(from: ChainEpoch, to: ChainEpoch) -> Vec<ChainEpoch> {
    let base_from = (from / LITE_INTERVAL) * LITE_INTERVAL;
    let base_to = (to / LITE_INTERVAL) * LITE_INTERVAL;
    (base_from..=base_to)
        .step_by(LITE_INTERVAL as usize)
        .collect()
}

/// Compute the required diff snapshot epochs for a given range.
fn compute_required_diff(from: ChainEpoch, to: ChainEpoch) -> Vec<ChainEpoch> {
    let base_from = (from / LITE_INTERVAL) * LITE_INTERVAL;
    let base_to = (to / LITE_INTERVAL) * LITE_INTERVAL;
    let diff_to = if to > base_to {
        ((to - 1) / DIFF_INTERVAL) * DIFF_INTERVAL
    } else if base_to >= DIFF_INTERVAL {
        base_to - DIFF_INTERVAL
    } else {
        // Range falls within the first lite segment with to on the boundary;
        // no diffs are needed (the lite snapshot at epoch 0 covers it).
        return Vec::new();
    };
    (base_from..=diff_to)
        .step_by(DIFF_INTERVAL as usize)
        .collect()
}

/// Return the subset of `required` epochs not present in `available`.
fn find_missing(required: &[ChainEpoch], available: &HashSet<ChainEpoch>) -> Vec<ChainEpoch> {
    required
        .iter()
        .filter(|h| !available.contains(h))
        .copied()
        .collect()
}

/// Fetch the set of available heights for a given network and snapshot type.
async fn fetch_available_heights(
    client: &reqwest::Client,
    network: &str,
    snapshot_type: &str,
) -> anyhow::Result<HashSet<ChainEpoch>> {
    let url = format!("{LIST_BASE}/{network}/{snapshot_type}?format=json");
    let resp = client
        .get(&url)
        .send()
        .await
        .context("failed to fetch archive listing")?;
    if !resp.status().is_success() {
        bail!("{url}: HTTP {}", resp.status());
    }
    let data: ListingResponse = resp
        .json()
        .await
        .context("failed to parse archive listing")?;
    Ok(parse_listing_heights(&data))
}

impl ArchiveMissingCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let (network, genesis_ts) = match &self.chain {
            NetworkChain::Mainnet => ("mainnet", MAINNET_GENESIS_TIMESTAMP),
            NetworkChain::Calibnet => ("calibnet", CALIBNET_GENESIS_TIMESTAMP),
            other => bail!("network {other} is not supported"),
        };

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let current_epoch =
            calculate_expected_epoch(now, genesis_ts, EPOCH_DURATION_SECONDS as u32);

        let from = self.from.unwrap_or(0);
        let to = self.to.unwrap_or_else(|| current_epoch - DIFF_INTERVAL);

        if from > to {
            bail!("--from ({from}) must be <= --to ({to})");
        }

        println!(
            "Checking {network} epochs {from}..={to} (current network epoch: {current_epoch})"
        );

        let client = global_http_client();

        println!("Fetching archive listings...");
        let (available_lite, available_diff) = tokio::try_join!(
            fetch_available_heights(&client, network, "lite"),
            fetch_available_heights(&client, network, "diff"),
        )?;

        println!(
            "Archive has {} lite and {} diff snapshots.",
            available_lite.len(),
            available_diff.len()
        );

        let required_lite = compute_required_lite(from, to);
        let required_diff = compute_required_diff(from, to);

        let missing_lite = find_missing(&required_lite, &available_lite);
        let missing_diff = find_missing(&required_diff, &available_diff);

        let total_required = required_lite.len() + required_diff.len();
        let total_missing = missing_lite.len() + missing_diff.len();

        if total_missing == 0 {
            let base_from = (from / LITE_INTERVAL) * LITE_INTERVAL;
            println!(
                "All {total_required} required snapshots are available (epochs {base_from}..={to}).",
            );
        } else {
            println!("\n{total_missing} of {total_required} required snapshots are MISSING:\n");
            if !missing_lite.is_empty() {
                println!("  Missing lite snapshots:");
                for h in &missing_lite {
                    println!("    lite at height {h}");
                }
            }
            if !missing_diff.is_empty() {
                println!("  Missing diff snapshots:");
                for h in &missing_diff {
                    println!("    diff at height {h}");
                }
            }
            bail!("{total_missing} of {total_required} required snapshots are missing");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_height_lite() {
        let url = Url::parse(
            "https://example.com/forest_snapshot_calibnet_2026-03-04_height_3510000.forest.car.zst",
        )
        .unwrap();
        assert_eq!(extract_height(&url), Some(3510000));
    }

    #[test]
    fn test_extract_height_diff() {
        let url = Url::parse(
            "https://example.com/forest_diff_calibnet_2022-11-02_height_0+3000.forest.car.zst",
        )
        .unwrap();
        assert_eq!(extract_height(&url), Some(0));
        let url = Url::parse(
            "https://example.com/forest_diff_mainnet_2025-12-24_height_3480000+3000.forest.car.zst",
        )
        .unwrap();
        assert_eq!(extract_height(&url), Some(3480000));
    }

    #[test]
    fn test_extract_height_invalid() {
        let url = Url::parse("https://example.com/not-a-snapshot").unwrap();
        assert_eq!(extract_height(&url), None);
    }

    #[test]
    fn test_compute_required_lite_single_segment() {
        // Range within one lite segment: only one lite snapshot needed.
        assert_eq!(compute_required_lite(30_000, 59_999), vec![30_000]);
    }

    #[test]
    fn test_compute_required_lite_multiple_segments() {
        assert_eq!(
            compute_required_lite(30_000, 90_000),
            vec![30_000, 60_000, 90_000]
        );
    }

    #[test]
    fn test_compute_required_lite_from_genesis() {
        assert_eq!(compute_required_lite(0, 60_000), vec![0, 30_000, 60_000]);
    }

    #[test]
    fn test_compute_required_lite_rounds_down() {
        // from=5000 rounds down to 0, to=35000 rounds down to 30000.
        assert_eq!(compute_required_lite(5_000, 35_000), vec![0, 30_000]);
    }

    #[test]
    fn test_compute_required_diff_within_segment() {
        // from=30000, to=36000 — need diffs from 30000 up to 33000.
        let diffs = compute_required_diff(30_000, 36_000);
        assert_eq!(diffs, vec![30_000, 33_000]);
    }

    #[test]
    fn test_compute_required_diff_exact_lite_boundary() {
        // to=60000 is exactly on a lite boundary — need all diffs in the
        // segment between 30000 and 60000.
        let diffs = compute_required_diff(30_000, 60_000);
        assert_eq!(
            diffs,
            vec![
                30_000, 33_000, 36_000, 39_000, 42_000, 45_000, 48_000, 51_000, 54_000, 57_000
            ]
        );
    }

    #[test]
    fn test_compute_required_diff_cross_segment() {
        // Spans two lite segments.
        let diffs = compute_required_diff(57_000, 63_000);
        // base_from=30000, base_to=60000, diff_to=60000
        assert_eq!(
            diffs,
            vec![
                30_000, 33_000, 36_000, 39_000, 42_000, 45_000, 48_000, 51_000, 54_000, 57_000,
                60_000
            ]
        );
    }

    #[test]
    fn test_find_missing_none() {
        let required = vec![0, 30_000, 60_000];
        let available: HashSet<_> = HashSet::from_iter([0, 30_000, 60_000, 90_000]);
        assert!(find_missing(&required, &available).is_empty());
    }

    #[test]
    fn test_find_missing_some() {
        let required = vec![0, 30_000, 60_000];
        let available: HashSet<_> = HashSet::from_iter([0, 60_000]);
        assert_eq!(find_missing(&required, &available), vec![30_000]);
    }

    #[test]
    fn test_find_missing_all() {
        let required = vec![0, 30_000];
        let available: HashSet<ChainEpoch> = HashSet::default();
        assert_eq!(find_missing(&required, &available), vec![0, 30_000]);
    }

    #[test]
    fn test_parse_listing_heights_from_json() {
        let json = r#"{
            "total": 3,
            "offset": 0,
            "limit": 0,
            "items": [
                {
                    "url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/lite/forest_snapshot_calibnet_2026-03-04_height_3510000.forest.car.zst",
                    "sha256url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/lite/forest_snapshot_calibnet_2026-03-04_height_3510000.forest.car.zst.sha256sum",
                    "size": 7528742793,
                    "uploaded": "2026-03-05T00:52:34.198Z"
                },
                {
                    "url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/lite/forest_snapshot_calibnet_2026-02-22_height_3480000.forest.car.zst",
                    "sha256url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/lite/forest_snapshot_calibnet_2026-02-22_height_3480000.forest.car.zst.sha256sum",
                    "size": 7440018317,
                    "uploaded": "2026-02-22T23:40:48.106Z"
                },
                {
                    "url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/lite/forest_snapshot_calibnet_2022-11-01_height_0.forest.car.zst",
                    "sha256url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/lite/forest_snapshot_calibnet_2022-11-01_height_0.forest.car.zst.sha256sum",
                    "size": 491234,
                    "uploaded": "2023-08-30T08:54:56.805Z"
                }
            ]
        }"#;
        let data: ListingResponse = serde_json::from_str(json).unwrap();
        let heights = parse_listing_heights(&data);
        assert_eq!(heights, HashSet::from_iter([0, 3_480_000, 3_510_000]));
    }

    #[test]
    fn test_parse_listing_heights_with_diffs() {
        let json = r#"{
            "total": 2,
            "offset": 0,
            "limit": 0,
            "items": [
                {
                    "url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/diff/forest_diff_calibnet_2026-03-04_height_3510000+3000.forest.car.zst",
                    "sha256url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/diff/forest_diff_calibnet_2026-03-04_height_3510000+3000.forest.car.zst.sha256sum",
                    "size": 123456,
                    "uploaded": "2026-03-05T01:00:00.000Z"
                },
                {
                    "url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/diff/forest_diff_calibnet_2022-11-02_height_0+3000.forest.car.zst",
                    "sha256url": "https://forest-archive.chainsafe.dev/archive/forest/calibnet/diff/forest_diff_calibnet_2022-11-02_height_0+3000.forest.car.zst.sha256sum",
                    "size": 789012,
                    "uploaded": "2023-08-30T09:00:00.000Z"
                }
            ]
        }"#;
        let data: ListingResponse = serde_json::from_str(json).unwrap();
        let heights = parse_listing_heights(&data);
        assert_eq!(heights, HashSet::from_iter([0, 3_510_000]));
    }

    #[test]
    fn test_end_to_end_missing_detection() {
        // Simulate checking calibnet epochs 0..=60000.
        // Available: lite at 0 and 60000 (missing 30000), all diffs present.
        let available_lite: HashSet<_> = HashSet::from_iter([0, 60_000]);
        let available_diff: HashSet<_> = (0..60_000).step_by(DIFF_INTERVAL as usize).collect();

        let required_lite = compute_required_lite(0, 60_000);
        let required_diff = compute_required_diff(0, 60_000);

        let missing_lite = find_missing(&required_lite, &available_lite);
        let missing_diff = find_missing(&required_diff, &available_diff);

        assert_eq!(missing_lite, vec![30_000]);
        assert!(missing_diff.is_empty());
    }

    #[test]
    fn test_end_to_end_all_present() {
        let available_lite: HashSet<_> = HashSet::from_iter([0, 30_000, 60_000]);
        let available_diff: HashSet<_> = (0..60_000).step_by(DIFF_INTERVAL as usize).collect();

        let required_lite = compute_required_lite(0, 60_000);
        let required_diff = compute_required_diff(0, 60_000);

        assert!(find_missing(&required_lite, &available_lite).is_empty());
        assert!(find_missing(&required_diff, &available_diff).is_empty());
    }
}
