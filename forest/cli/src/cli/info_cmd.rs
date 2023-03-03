// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{string::ToString, time::Instant};

use clap::Subcommand;
use colored::*;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_rpc_client::{
    chain_get_name, chain_get_tipset, chain_head, state_start_time, wallet_default_address,
};

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
    behind: u64,
    health: usize,
    epoch: i64,
    base_fee: String,
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
    let behind = delta / 30;

    let sync_status = if delta < 30 * 3 / 2 {
        SyncStatus::Ok
    } else if delta < 30 * 5 {
        SyncStatus::Slow
    } else {
        SyncStatus::Behind
    };

    let base_fee = chain_head.0.min_ticket_block().parent_base_fee();
    let base_fee = base_fee.to_string();

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
        let start_time = state_start_time(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let network = chain_get_name((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let behind = node_status.behind;
        let health = node_status.health;
        let base_fee = node_status.base_fee;
        let sync_status = node_status.sync_status;
        let epoch = node_status.epoch;

        let chain_status = format!(
            "[sync: {sync_status}! ({behind} behind)] [basefee: {base_fee} pFIL] [epoch: {epoch}]"
        )
        .blue();

        println!("Network: {}", network.green());
        println!("Uptime: {start_time}");
        println!("Chain: {chain_status}");

        match health {
            0..=85 => {
                let chain_health = format!("{health}%\n\n").red();
                println!("Chain health: {chain_health}");
            }
            (86..) => {
                let chain_health = format!("{health}%\n\n").green();
                println!("Chain health: {chain_health}");
            }
            _ => {}
        }

        // Wallet info
        let default_wallet_address = wallet_default_address((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;
        let default_wallet_address = default_wallet_address.bold();
        println!("Default wallet address: {default_wallet_address}");

        Ok(())
    }
}
