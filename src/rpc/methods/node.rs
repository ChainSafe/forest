// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{
    lotus_json::lotus_json_with_self,
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
};
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub enum NodeStatus {}
impl RpcMethod<0> for NodeStatus {
    const NAME: &'static str = "Filecoin.NodeStatus";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = NodeStatusResult;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut node_status = NodeStatusResult::default();

        let head = ctx.chain_store().heaviest_tipset();
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

        let chain_finality = ctx.chain_config().policy.chain_finality;

        node_status.sync_status.epoch = head.epoch() as u64;
        node_status.sync_status.behind = behind;

        if head.epoch() > chain_finality {
            let mut block_count = 0;
            let mut ts = head;

            for _ in 0..100 {
                block_count += ts.block_headers().len();
                let tsk = ts.parents();
                ts = ctx.chain_index().load_required_tipset(tsk)?;
            }

            node_status.chain_status.blocks_per_tipset_last_100 = block_count as f64 / 100.;

            for _ in 100..chain_finality {
                block_count += ts.block_headers().len();
                let tsk = ts.parents();
                ts = ctx.chain_index().load_required_tipset(tsk)?;
            }

            node_status.chain_status.blocks_per_tipset_last_finality =
                block_count as f64 / chain_finality as f64;
        }

        Ok(node_status)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, Clone, JsonSchema)]
pub struct NodeSyncStatus {
    pub epoch: u64,
    pub behind: u64,
}
lotus_json_with_self!(NodeSyncStatus);

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, Clone, JsonSchema)]
pub struct NodePeerStatus {
    pub peers_to_publish_msgs: u32,
    pub peers_to_publish_blocks: u32,
}
lotus_json_with_self!(NodePeerStatus);

#[derive(Debug, PartialEq, Serialize, Deserialize, Default, Clone, JsonSchema)]
pub struct NodeChainStatus {
    pub blocks_per_tipset_last_100: f64,
    pub blocks_per_tipset_last_finality: f64,
}
lotus_json_with_self!(NodeChainStatus);

#[derive(Debug, Deserialize, Default, Serialize, Clone, JsonSchema, PartialEq)]
pub struct NodeStatusResult {
    pub sync_status: NodeSyncStatus,
    pub peer_status: NodePeerStatus,
    pub chain_status: NodeChainStatus,
}
lotus_json_with_self!(NodeStatusResult);
