// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::rpc::error::ServerError;
use crate::rpc::Ctx;
use fvm_ipld_blockstore::Blockstore;

pub const NODE_STATUS: &str = "Filecoin.NodeStatus";
pub type NodeStatusResult = NodeStatus;

use serde::{Deserialize, Serialize};

use crate::lotus_json::lotus_json_with_self;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct NodeSyncStatus {
    pub epoch: u64,
    pub behind: u64,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct NodePeerStatus {
    pub peers_to_publish_msgs: u32,
    pub peers_to_publish_blocks: u32,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct NodeChainStatus {
    pub blocks_per_tipset_last_100: f64,
    pub blocks_per_tipset_last_finality: f64,
}

#[derive(Debug, Deserialize, Default, Serialize, Clone)]
pub struct NodeStatus {
    pub sync_status: NodeSyncStatus,
    pub peer_status: NodePeerStatus,
    pub chain_status: NodeChainStatus,
}

lotus_json_with_self!(NodeStatus);

pub async fn node_status<DB: Blockstore>(data: Ctx<DB>) -> Result<NodeStatusResult, ServerError> {
    let mut node_status = NodeStatusResult::default();

    let head = data.state_manager.chain_store().heaviest_tipset();
    let cur_duration: Duration = SystemTime::now().duration_since(UNIX_EPOCH)?;

    let ts = head.min_timestamp();
    let cur_duration_secs = cur_duration.as_secs();
    let behind = if ts <= cur_duration_secs + 1 {
        cur_duration_secs.saturating_sub(ts)
    } else {
        return Err(anyhow::anyhow!(
            "System time should not be behind tipset timestamp, please sync the system clock."
        )
        .into());
    };

    let chain_finality = data.state_manager.chain_config().policy.chain_finality;

    node_status.sync_status.epoch = head.epoch() as u64;
    node_status.sync_status.behind = behind;

    if head.epoch() > chain_finality {
        let mut block_count = 0;
        let mut ts = head;

        for _ in 0..100 {
            block_count += ts.block_headers().len();
            let tsk = ts.parents();
            ts = data.chain_store.chain_index.load_required_tipset(tsk)?;
        }

        node_status.chain_status.blocks_per_tipset_last_100 = block_count as f64 / 100.;

        for _ in 100..chain_finality {
            block_count += ts.block_headers().len();
            let tsk = ts.parents();
            ts = data.chain_store.chain_index.load_required_tipset(tsk)?;
        }

        node_status.chain_status.blocks_per_tipset_last_finality =
            block_count as f64 / chain_finality as f64;
    }

    Ok(node_status)
}
