// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Instant;

use clap::Subcommand;
use colored::*;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_rpc_client::{
    chain_get_name, chain_get_tipset, chain_head, start_time, wallet_default_address,
};
use forest_shim::econ::TokenAmount;
use forest_utils::io::parser::FormattingMode;
use time::OffsetDateTime;

use super::Config;
use crate::cli::{handle_rpc_err, wallet_cmd::format_balance_string};

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
    /// Chain health calculated as percentage:
    /// amount of blocks in last finality /
    /// very healthy amount of blocks in a finality (900 epochs * 5 blocks per
    /// tipset)
    health: usize,
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

    let sync_status = if delta < 30 * 3 / 2 {
        SyncStatus::Ok
    } else if delta < 30 * 5 {
        SyncStatus::Slow
    } else {
        SyncStatus::Behind
    };

    let base_fee = chain_head.0.min_ticket_block().parent_base_fee().clone();

    // chain health
    let blocks_per_tipset_last_finality = if epoch > chain_finality {
        let mut block_count = 0;
        let mut ts = chain_head.0;

        for _ in 0..chain_finality {
            block_count += ts.blocks().len();
            let tsk = ts.parents();
            let tsk = TipsetKeysJson(tsk.clone());
            if let Ok(tsjson) = chain_get_tipset((tsk,), &config.client.rpc_token).await {
                ts = tsjson.0;
            }
        }

        block_count / chain_finality as usize
    } else {
        0
    };

    let health = 100 * (900 * blocks_per_tipset_last_finality) / (900 * 5);

    Ok(NodeStatusInfo {
        behind,
        health,
        epoch,
        base_fee,
        sync_status,
    })
}

impl InfoCommand {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        let node_status = node_status(&config).await?;

        // uptime
        let start_time = start_time(&config.client.rpc_token)
            .await
            .map(|t| {
                let start_time = t.to_hms();
                format!(
                    "{}h {}m {}s (Started at: {})",
                    start_time.0, start_time.1, start_time.2, t
                )
            })
            .map_err(handle_rpc_err)?;

        let network = chain_get_name((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let behind = OffsetDateTime::from_unix_timestamp(node_status.behind as i64)?.to_hms();
        let health = node_status.health;
        let base_fee =
            format_balance_string(node_status.base_fee, FormattingMode::NotExactNotFixed)?;
        let sync_status = node_status.sync_status;
        let epoch = node_status.epoch;

        let behind_time = format!("{}h {}m {}s", behind.0, behind.1, behind.2);

        let chain_status = format!(
            "[sync: {sync_status}! ({behind_time} behind)] [basefee: {base_fee}] [epoch: {epoch}]"
        )
        .blue();

        println!("Network: {}", network.green());
        println!("Uptime: {start_time}");
        println!("Chain: {chain_status}");

        let mut chain_health = format!("{health}%\n\n").red();
        if health > 85 {
            chain_health = chain_health.green();
        }
        println!("Chain health: {chain_health}");

        // Wallet info
        let default_wallet_address = wallet_default_address((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;
        println!("Default wallet address: {}", default_wallet_address.bold());

        Ok(())
    }
}
