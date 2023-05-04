// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
use clap::Subcommand;
use colored::*;
use forest_blocks::{tipset_keys_json::TipsetKeysJson, Tipset};
use forest_cli_shared::{cli::CliOpts, logger::LoggingColor};
use forest_rpc_client::{
    chain_get_name, chain_get_tipset, chain_head, start_time, wallet_default_address,
};
use forest_shim::econ::TokenAmount;
use forest_utils::io::parser::{format_balance_string, FormattingMode};
use fvm_shared::{clock::EPOCH_DURATION_SECONDS, BLOCKS_PER_EPOCH};

use super::Config;
use crate::cli::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    Show,
}

#[derive(Debug, strum_macros::Display, PartialEq)]
enum SyncStatus {
    Ok,
    Slow,
    Behind,
}

#[derive(Debug)]
pub struct NodeStatusInfo {
    /// timestamp of how far behind the node is with respect to syncing to head
    behind: u64,
    /// Chain health is the percentage denoting how close we are to having an
    /// average of 5 blocks per tipset in the last couple of hours.
    /// The number of blocks per tipset is non-deterministic but averaging at 5
    /// is considered healthy.
    health: f64,
    /// epoch the node is currently at
    epoch: i64,
    /// base fee
    base_fee: TokenAmount,
    /// sync status information
    sync_status: SyncStatus,
}

fn get_node_status(
    chain_head: &Arc<Tipset>,
    block_count: usize,
    num_tipsets: u64,
) -> anyhow::Result<NodeStatusInfo> {
    let epoch = chain_head.epoch();
    let ts = chain_head.min_timestamp();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    // log::info!("ts: {ts}, now: {now}");
    let delta = if ts > now {
        // Allows system time to be 1 second slower
        if ts <= now + 1 {
            0
        } else {
            anyhow::bail!(
                "System time should not be behind tipset timestamp, please sync the system clock."
            );
        }
    } else {
        now - ts
    };

    let behind = delta;
    let sync_status = if delta < EPOCH_DURATION_SECONDS as u64 * 3 / 2 {
        // within 1.5 epochs
        SyncStatus::Ok
    } else if delta < EPOCH_DURATION_SECONDS as u64 * 5 {
        // within 5 epochs
        SyncStatus::Slow
    } else {
        SyncStatus::Behind
    };

    let base_fee = chain_head.min_ticket_block().parent_base_fee().clone();

    let health = (100 * block_count) as f64 / (num_tipsets * BLOCKS_PER_EPOCH) as f64;

    log::debug!(
        "[Health data] health: {health}, block_count: {block_count}, num_tipsets: {num_tipsets}"
    );

    Ok(NodeStatusInfo {
        behind,
        health,
        epoch,
        base_fee,
        sync_status,
    })
}

impl InfoCommand {
    pub async fn run(&self, config: Config, opts: &CliOpts) -> anyhow::Result<()> {
        // uptime
        let start_time = start_time(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let network = chain_get_name((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        // Wallet info
        let default_wallet_address = wallet_default_address((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let chain_head = chain_head(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)
            .context("couldn't fetch chain head, is the node running?")?;

        let mut ts = chain_head.0.clone();

        let mut num_tipsets = 1;
        let mut block_count = ts.blocks().len();

        for _ in 0..(config.chain.policy.chain_finality - 1).min(ts.epoch()) {
            let parent_tipset_keys = TipsetKeysJson(ts.parents().clone());
            let tsjson = chain_get_tipset((parent_tipset_keys,), &config.client.rpc_token)
                .await
                .map_err(handle_rpc_err)
                .context("Failed to fetch tipset.")?;
            ts = tsjson.0;
            num_tipsets += 1;
            block_count += ts.blocks().len();
        }

        let node_status = get_node_status(&chain_head.0, block_count, num_tipsets)?;
        let info = fmt_info(
            &node_status,
            start_time,
            &network,
            &default_wallet_address,
            &opts.color,
        )?;

        println!("Network: {}", info.network);
        println!("Uptime: {}", info.uptime);
        println!("Chain: {}", info.chain_status);
        println!("Chain health: {}", info.health);
        println!("Default wallet address: {}", info.wallet_address);

        Ok(())
    }
}

struct NodeInfoOutput {
    chain_status: ColoredString,
    network: ColoredString,
    uptime: String,
    health: ColoredString,
    wallet_address: ColoredString,
}

fn chain_status(node_status: &NodeStatusInfo) -> anyhow::Result<String> {
    let NodeStatusInfo {
        behind,
        epoch,
        base_fee,
        sync_status,
        ..
    } = node_status;
    let base_fee = format_balance_string(base_fee.clone(), FormattingMode::NotExactNotFixed)?;
    let behind = {
        let b = Utc.timestamp_millis_opt(*behind as i64).unwrap();
        format!("{}h {}m {}s", b.hour(), b.minute(), b.second())
    };
    Ok(format!(
        "[sync: {sync_status}! ({behind} behind)] [basefee: {base_fee}] [epoch: {epoch}]"
    ))
}

fn fmt_info(
    node_status: &NodeStatusInfo,
    start_time: DateTime<Local>,
    network: &str,
    default_wallet_address: &Option<String>,
    color: &LoggingColor,
) -> anyhow::Result<NodeInfoOutput> {
    let NodeStatusInfo { health, .. } = node_status;

    let use_color = color.coloring_enabled();
    let uptime = {
        format!(
            "{}h {}m {}s (Started at: {})",
            start_time.hour(),
            start_time.minute(),
            start_time.second(),
            start_time
        )
    };

    let chain_status = {
        let status = chain_status(node_status)?;
        if use_color {
            status.blue()
        } else {
            status.normal()
        }
    };

    let network = if use_color {
        network.green()
    } else {
        network.normal()
    };

    let health = {
        let s = format!("{health:.2}%\n\n");
        if use_color {
            if *health > 85. {
                s.green()
            } else {
                s.red()
            }
        } else {
            s.normal()
        }
    };

    let default_wallet_address = {
        let addr = default_wallet_address.clone().unwrap_or("-".to_string());
        if use_color {
            addr.bold()
        } else {
            addr.normal()
        }
    };

    Ok(NodeInfoOutput {
        chain_status,
        network,
        uptime,
        health,
        wallet_address: default_wallet_address,
    })
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use chrono::Local;
    use colored::*;
    use forest_blocks::{BlockHeader, Tipset};
    use forest_cli_shared::logger::LoggingColor;
    use forest_shim::{address::Address, econ::TokenAmount};

    use super::{fmt_info, get_node_status, NodeStatusInfo};
    use crate::cli::info_cmd::{chain_status, SyncStatus};

    fn node_status_good() -> NodeStatusInfo {
        super::NodeStatusInfo {
            behind: 0,
            health: 90.,
            epoch: i64::MAX,
            base_fee: TokenAmount::from_whole(1),
            sync_status: SyncStatus::Ok,
        }
    }

    fn node_status_bad() -> NodeStatusInfo {
        super::NodeStatusInfo {
            behind: 0,
            health: 0.,
            epoch: 0,
            base_fee: TokenAmount::from_whole(1),
            sync_status: SyncStatus::Behind,
        }
    }

    #[test]
    fn node_status_with_null_tipset() {
        let mock_header = BlockHeader::builder()
            .miner_address(Address::from_str("f2kmbjvz7vagl2z6pfrbjoggrkjofxspp7cqtw2zy").unwrap())
            .build()
            .unwrap();
        let tipset = Tipset::from(&mock_header);
        let node_status = get_node_status(&Arc::new(tipset), 0, 0).unwrap();
        assert!(node_status.health.is_nan());
        assert_eq!(node_status.sync_status, SyncStatus::Behind);
    }

    #[test]
    fn test_node_info_formattting() {
        // no color tests
        let color = LoggingColor::Never;
        let node_status = node_status_bad();
        let start_time = Local::now();
        let default_wallet_address = Some("-".to_string());
        let network = "calibnet";
        let info = fmt_info(
            &node_status,
            start_time,
            &network,
            &default_wallet_address,
            &color,
        )
        .unwrap();

        assert_eq!(info.network, "calibnet".normal());
        assert_eq!(info.health, "0.00%\n\n".normal());
        assert_eq!(info.wallet_address, "-".normal());
        let s = chain_status(&node_status).unwrap();
        assert_eq!(info.chain_status, s.normal());

        // with color tests
        let color = LoggingColor::Always;
        let node_status = node_status_good();
        let info = fmt_info(
            &node_status,
            start_time,
            &network,
            &default_wallet_address,
            &color,
        )
        .unwrap();
        assert_eq!(info.network, "calibnet".green());
        assert_eq!(info.health, "90.00%\n\n".green());
        assert_eq!(info.wallet_address, "-".bold());
        let s = chain_status(&node_status).unwrap();
        assert_eq!(info.chain_status, s.blue());
    }
}
