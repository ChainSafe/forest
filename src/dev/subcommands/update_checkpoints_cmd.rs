// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context as _;
use cid::Cid;
use clap::Parser;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

use crate::rpc::Client;
use crate::rpc::prelude::*;
use crate::rpc::types::ApiTipsetKey;
use crate::shim::clock::ChainEpoch;

/// The interval between checkpoints (86400 epochs = 1 day at 30s block time)
const CHECKPOINT_INTERVAL: ChainEpoch = 86400;

/// YAML structure for known_blocks.yaml
/// Using IndexMap to preserve insertion order
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KnownBlocks {
    #[serde(with = "cid_string_map")]
    calibnet: IndexMap<ChainEpoch, Cid>,
    #[serde(with = "cid_string_map")]
    mainnet: IndexMap<ChainEpoch, Cid>,
}

/// Network selection for checkpoint updates
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Network {
    /// Update both calibnet and mainnet
    All,
    /// Update calibnet only
    Calibnet,
    /// Update mainnet only
    Mainnet,
}

/// Update known blocks in build/known_blocks.yaml by querying RPC endpoints
///
/// This command finds and adds missing checkpoint entries at constant intervals
/// by querying Filfox or other full-archive RPC nodes that support historical queries.
#[derive(Debug, Parser)]
pub struct UpdateCheckpointsCommand {
    /// Path to known_blocks.yaml file
    #[arg(long, default_value = "build/known_blocks.yaml")]
    known_blocks_file: PathBuf,

    /// Mainnet RPC endpoint (Filfox recommended for full historical data)
    #[arg(long, default_value = "https://filfox.info")]
    mainnet_rpc: Url,

    /// Calibnet RPC endpoint (Filfox recommended for full historical data)
    #[arg(long, default_value = "https://calibration.filfox.info")]
    calibnet_rpc: Url,

    /// Which network(s) to update
    #[arg(long, default_value = "all")]
    network: Network,

    /// Dry run - don't write changes to file
    #[arg(long)]
    dry_run: bool,
}

impl UpdateCheckpointsCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            known_blocks_file,
            mainnet_rpc,
            calibnet_rpc,
            network,
            dry_run,
        } = self;

        println!("Reading known blocks from: {}", known_blocks_file.display());
        let yaml_content = std::fs::read_to_string(&known_blocks_file)
            .context("Failed to read known_blocks.yaml")?;
        let mut known_blocks: KnownBlocks =
            serde_yaml::from_str(&yaml_content).context("Failed to parse known_blocks.yaml")?;

        if matches!(network, Network::All | Network::Calibnet) {
            println!("\n=== Updating Calibnet Checkpoints ===");
            let calibnet_client = Client::from_url(calibnet_rpc);
            update_chain_checkpoints(&calibnet_client, &mut known_blocks.calibnet, "calibnet")
                .await?;
        }

        if matches!(network, Network::All | Network::Mainnet) {
            println!("\n=== Updating Mainnet Checkpoints ===");
            let mainnet_client = Client::from_url(mainnet_rpc);
            update_chain_checkpoints(&mainnet_client, &mut known_blocks.mainnet, "mainnet").await?;
        }

        if dry_run {
            println!("\n=== Dry Run - Changes Not Written ===");
            println!("Would write to: {}", known_blocks_file.display());
        } else {
            println!("\n=== Writing Updated Checkpoints ===");
            write_known_blocks(&known_blocks_file, &known_blocks)?;
            println!("Successfully updated: {}", known_blocks_file.display());
        }

        Ok(())
    }
}

async fn update_chain_checkpoints(
    client: &Client,
    checkpoints: &mut IndexMap<ChainEpoch, Cid>,
    chain_name: &str,
) -> anyhow::Result<()> {
    println!("Fetching chain head for {chain_name}...");
    let head = ChainHead::call(client, ())
        .await
        .context("Failed to get chain head")?;

    let current_epoch = head.epoch();
    println!("Current epoch: {}", current_epoch);

    let latest_checkpoint_epoch = (current_epoch / CHECKPOINT_INTERVAL) * CHECKPOINT_INTERVAL;

    let existing_max_epoch = checkpoints.keys().max().copied().unwrap_or(0);
    println!("Existing max checkpoint epoch: {}", existing_max_epoch);
    println!(
        "Latest checkpoint epoch should be: {}",
        latest_checkpoint_epoch
    );

    if latest_checkpoint_epoch <= existing_max_epoch {
        println!("No new checkpoints needed (already up to date)");
        return Ok(());
    }

    let mut needed_epochs = Vec::new();
    let mut epoch = existing_max_epoch + CHECKPOINT_INTERVAL;
    while epoch <= latest_checkpoint_epoch {
        if !checkpoints.contains_key(&epoch) {
            needed_epochs.push(epoch);
        }
        epoch += CHECKPOINT_INTERVAL;
    }

    if needed_epochs.is_empty() {
        println!("No missing checkpoints to add");
        return Ok(());
    }

    println!("Need to add {} checkpoint(s)", needed_epochs.len());

    println!("Fetching checkpoints via RPC...");
    let mut found_checkpoints: IndexMap<ChainEpoch, Cid> = IndexMap::new();

    for &requested_epoch in &needed_epochs {
        match fetch_checkpoint_at_height(client, requested_epoch).await {
            Ok((actual_epoch, cid)) => {
                found_checkpoints.insert(actual_epoch, cid);

                if actual_epoch != requested_epoch {
                    println!(
                        "  ✓ Epoch {actual_epoch} (requested {requested_epoch}, no blocks at exact height): {cid}"
                    );
                } else {
                    println!("  ✓ Epoch {}: {}", actual_epoch, cid);
                }

                // Map chain name for Beryx URL (calibnet -> calibration)
                let beryx_network = if chain_name == "calibnet" {
                    "calibration"
                } else {
                    chain_name
                };
                println!("    Verify at: https://beryx.io/fil/{beryx_network}/block-cid/{cid}",);
            }
            Err(e) => {
                println!("  ✗ Epoch {requested_epoch}: {e}");
            }
        }
    }

    let num_found = found_checkpoints.len();
    println!("\nAdding {num_found} new checkpoint(s) to the file...");

    let mut sorted_checkpoints: Vec<_> = found_checkpoints.into_iter().collect();
    sorted_checkpoints.sort_by_key(|(epoch, _)| std::cmp::Reverse(*epoch));

    let mut new_map = IndexMap::new();
    for (epoch, cid) in sorted_checkpoints {
        new_map.insert(epoch, cid);
    }
    new_map.extend(checkpoints.drain(..));
    *checkpoints = new_map;

    if num_found < needed_epochs.len() {
        anyhow::bail!(
            "Only found {num_found} out of {} needed checkpoints. Consider using an RPC provider with full historical data (e.g., Filfox).",
            needed_epochs.len()
        );
    }

    Ok(())
}

/// Fetch a checkpoint at a specific height via RPC
/// Returns (actual_epoch, cid) where actual_epoch might be slightly earlier than requested
/// if there were no blocks at the exact requested height.
async fn fetch_checkpoint_at_height(
    client: &Client,
    epoch: ChainEpoch,
) -> anyhow::Result<(ChainEpoch, Cid)> {
    let tipset = ChainGetTipSetByHeight::call(client, (epoch, ApiTipsetKey(None)))
        .await
        .context("ChainGetTipSetByHeight RPC call failed")?;

    let actual_epoch = tipset.epoch();
    let first_block_cid = tipset.block_headers().first().cid();
    Ok((actual_epoch, *first_block_cid))
}

fn write_known_blocks(path: &PathBuf, known_blocks: &KnownBlocks) -> anyhow::Result<()> {
    let mut output = String::new();

    output.push_str("# This file is auto-generated by `forest-dev update-checkpoints` command.\n");
    output.push_str("# Do not edit manually. Run the command to update checkpoints.\n\n");

    output.push_str("calibnet:\n");
    for (epoch, cid) in &known_blocks.calibnet {
        output.push_str(&format!("  {epoch}: {cid}\n"));
    }

    output.push_str("mainnet:\n");
    for (epoch, cid) in &known_blocks.mainnet {
        output.push_str(&format!("  {epoch}: {cid}\n"));
    }

    std::fs::write(path, output).context(format!(
        "Failed to write updated known blocks to {}",
        path.display()
    ))?;

    Ok(())
}

// Custom serde module for serializing/deserializing IndexMap<ChainEpoch, Cid> as strings
mod cid_string_map {
    use super::*;
    use serde::de::{Deserialize, Deserializer};
    use serde::ser::Serializer;
    use std::str::FromStr;

    pub fn serialize<S>(map: &IndexMap<ChainEpoch, Cid>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut ser_map = serializer.serialize_map(Some(map.len()))?;
        for (k, v) in map {
            ser_map.serialize_entry(k, &v.to_string())?;
        }
        ser_map.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<IndexMap<ChainEpoch, Cid>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string_map: IndexMap<ChainEpoch, String> = IndexMap::deserialize(deserializer)?;
        string_map
            .into_iter()
            .map(|(k, v)| {
                Cid::from_str(&v)
                    .map(|cid| (k, cid))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}
