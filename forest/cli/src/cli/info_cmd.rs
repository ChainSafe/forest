use std::time::Instant;

use clap::Subcommand;
use colored::*;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_cli_shared::cli::cli_error_and_die;
use forest_rpc_client::{
    chain_get_name, chain_get_tipset, chain_head, state_start_time, sync_status,
    wallet_default_address,
};

use super::Config;
use crate::cli::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    Show,
}

#[derive(Debug)]
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

const CHAIN_FINALITY: i64 = 900;

pub async fn node_status(config: &Config) -> NodeStatusInfo {
    let chain_head = match chain_head(&config.client.rpc_token).await {
        Ok(head) => head.0,
        Err(_) => cli_error_and_die("Could not get network head. Is the node running?", 1),
    };

    let epoch = chain_head.epoch();
    let ts = chain_head.min_timestamp();
    let now = Instant::now().elapsed().as_secs();
    let delta = now - ts;
    let behind = delta / 30;

    let sync_status = if delta < 30 * 3 / 2 {
        SyncStatus::Ok
    } else if delta < 30 * 5 {
        SyncStatus::Slow
    } else {
        SyncStatus::Behind
    };

    let base_fee = chain_head.min_ticket_block().parent_base_fee();
    // if base_fee > 7000_000_000 { // 7 nFIL

    // } else if base_fee > 3000_000_000 {

    // }

    let base_fee = base_fee.to_string();

    // chain health
    let blocks_per_tipset_last_finality = if epoch > CHAIN_FINALITY {
        let mut block_count = 0;
        let mut ts = chain_head;

        for i in 0..100 {
            block_count += ts.blocks().len();
            let tsk = ts.parents();
            let tsk = TipsetKeysJson(tsk.clone());
            if let Ok(tsjson) = chain_get_tipset((tsk,), &config.client.rpc_token).await {
                ts = tsjson.0;
            }
        }

        for i in 100..CHAIN_FINALITY {
            block_count += ts.blocks().len();
            let tsk = ts.parents();
            let tsk = TipsetKeysJson(tsk.clone());
            if let Ok(tsjson) = chain_get_tipset((tsk,), &config.client.rpc_token).await {
                ts = tsjson.0;
            }
        }

        block_count / CHAIN_FINALITY as usize
    } else {
        0
    };

    let health = 100 * (900 * blocks_per_tipset_last_finality) / (900 * 5);

    NodeStatusInfo {
        behind,
        health,
        epoch,
        base_fee,
        sync_status,
    }
}

impl InfoCommand {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        let node_status = node_status(&config).await;

        // chain sync status
        let response = sync_status((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let state = &response.active_syncs[0];
        let epoch = state.epoch();
        let start_time = state_start_time(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;
        let network = chain_get_name((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let default_wallet_address = wallet_default_address((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let behind = node_status.behind;
        let health = node_status.health;
        let base_fee = node_status.base_fee;
        let sync_status = node_status.sync_status;

        let chain_status = format!(
            "[sync {:?}! ({behind} behind)] [basefee {base_fee} pFIL] [epoch {epoch}]",
            sync_status
        )
        .blue();

        println!("Network: {}", network.green());
        println!("Start time: {}", start_time);
        println!("Chain state: {}", chain_status);

        if health > 85 {
            let chain_health = format!("Chain health: {}%\n\n", health).green();
            println!("{chain_health}");
        } else if health < 85 {
            let chain_health = format!("Chain health: {}%\n\n", health).red();
            println!("{chain_health}");
        }

        // Wallet info
        let default_wallet_address = format!("{default_wallet_address}").bold();
        println!("Default wallet address: {}", default_wallet_address);

        Ok(())
    }
}

