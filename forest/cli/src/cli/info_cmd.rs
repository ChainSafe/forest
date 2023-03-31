// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Instant;

use clap::Subcommand;
use colored::*;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_cli_shared::{cli::CliOpts, logger::LoggingColor};
use forest_rpc_client::{
    chain_get_name, chain_get_tipset, chain_head, start_time, wallet_default_address,
};
use forest_shim::econ::TokenAmount;
use forest_utils::io::parser::{format_balance_string, FormattingMode};
use fvm_shared::{clock::EPOCH_DURATION_SECONDS, BLOCKS_PER_EPOCH};
use time::OffsetDateTime;

use super::Config;
use crate::cli::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    Show,
}

#[derive(Debug, strum_macros::Display)]
enum SyncStatus {
    Ok,
    Slow,
    Behind,
}

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

pub async fn node_status(config: &Config) -> anyhow::Result<NodeStatusInfo, anyhow::Error> {
    let chain_head = chain_head(&config.client.rpc_token)
        .await
        .map_err(|_| anyhow::Error::msg("couldn't fetch chain head, is the node running?"))?;

    let chain_finality = config.chain.policy.chain_finality;
    let epoch = chain_head.0.epoch();
    let ts = chain_head.0.min_timestamp();
    let now = Instant::now().elapsed().as_secs();
    let delta = ts - now;
    let behind = delta;

    let sync_status = if delta < EPOCH_DURATION_SECONDS as u64 * 3 / 2 {
        // within 1.5 epochs
        SyncStatus::Ok
    } else if delta < 30 * 5 {
        // within 5 epochs
        SyncStatus::Slow
    } else {
        SyncStatus::Behind
    };

    let base_fee = chain_head.0.min_ticket_block().parent_base_fee().clone();

    // chain health
    let mut num_tipsets = 1;
    let mut block_count = 0;

    let mut ts = chain_head.0;
    for _ in 0..chain_finality {
        block_count += ts.blocks().len();
        let tsk = ts.parents();
        let tsk = TipsetKeysJson(tsk.clone());
        if let Ok(tsjson) = chain_get_tipset((tsk,), &config.client.rpc_token).await {
            ts = tsjson.0;
            num_tipsets += 1;
        }
    }

    let blocks_per_tipset_last_finality = if epoch > chain_finality {
        block_count as f64 / chain_finality as f64
    } else {
        block_count as f64 / num_tipsets as f64
    };

    let health = 100. * blocks_per_tipset_last_finality / BLOCKS_PER_EPOCH as f64;

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
            .map_err(handle_rpc_err)?
            .map(|default_wallet_address| default_wallet_address);

        display_info(
            &node_status(&config).await?,
            start_time,
            &network,
            default_wallet_address,
            &opts.color,
        )?;

        Ok(())
    }
}

fn display_info(
    node_status: &NodeStatusInfo,
    start_time: OffsetDateTime,
    network: &str,
    default_wallet_address: Option<String>,
    color: &LoggingColor,
) -> anyhow::Result<()> {
    let NodeStatusInfo {
        health,
        behind,
        epoch,
        base_fee,
        sync_status,
    } = node_status;

    let use_color = color.coloring_enabled();

    let start_time = {
        let st = start_time.to_hms();
        format!("{}h {}m {}s (Started at: {})", st.0, st.1, st.2, start_time)
    };

    let base_fee = format_balance_string(base_fee.clone(), FormattingMode::NotExactNotFixed)?;
    let behind = {
        let b = OffsetDateTime::from_unix_timestamp(*behind as i64)?.to_hms();
        format!("{}h {}m {}s", b.0, b.1, b.2)
    };
    let chain_status =
        format!("[sync: {sync_status}! ({behind} behind)] [basefee: {base_fee}] [epoch: {epoch}]");

    let chain_status = if use_color {
        chain_status.blue()
    } else {
        chain_status.normal()
    };

    println!(
        "Network: {}",
        if use_color {
            network.green()
        } else {
            network.normal()
        }
    );
    println!("Uptime: {start_time}");
    println!("Chain: {chain_status}");

    let chain_health = {
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

    println!("Chain health: {chain_health}");

    let default_wallet_address = default_wallet_address.unwrap_or("-".to_string());
    println!(
        "Default wallet address: {}",
        if use_color {
            default_wallet_address.bold()
        } else {
            default_wallet_address.normal()
        }
    );

    Ok(())
}
