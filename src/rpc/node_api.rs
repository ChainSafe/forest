// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::rpc::error::JsonRpcError;
use crate::rpc::Ctx;
use crate::rpc_api::node_api::NodeStatusResult;
use fvm_ipld_blockstore::Blockstore;

pub async fn node_status<DB: Blockstore>(data: Ctx<DB>) -> Result<NodeStatusResult, JsonRpcError> {
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
