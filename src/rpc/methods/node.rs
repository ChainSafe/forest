// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{
    lotus_json::lotus_json_with_self,
    networks::calculate_expected_epoch,
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
    shim::clock::EPOCH_DURATION_SECONDS,
};
use anyhow::ensure;
use enumflags2::BitFlags;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Epochs the node is behind the head tipset, matching Lotus' `NodeStatus`
/// (`Behind` is in epochs, not seconds). Clock skew of up to one epoch (head
/// ahead of the local clock) clamps to `0`; beyond that the value is
/// meaningless, so an error is returned instead.
fn epochs_behind_head(head_timestamp: u64, now_secs: u64) -> anyhow::Result<u64> {
    ensure!(
        head_timestamp <= now_secs + EPOCH_DURATION_SECONDS as u64,
        "Head tipset timestamp is more than one epoch ahead of system time, please sync the system clock."
    );
    Ok(calculate_expected_epoch(now_secs, head_timestamp, EPOCH_DURATION_SECONDS as u32) as u64)
}

pub enum NodeStatus {}
impl RpcMethod<1> for NodeStatus {
    const NAME: &'static str = "Filecoin.NodeStatus";
    const PARAM_NAMES: [&'static str; 1] = ["inclChainStatus"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str =
        "Returns the node's status, including sync and chain health information.";

    type Params = (bool,);
    type Ok = NodeStatusResult;

    async fn handle(
        ctx: Ctx,
        (incl_chain_status,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut node_status = NodeStatusResult::default();

        let head = ctx.chain_store().heaviest_tipset();
        let cur_duration: Duration = SystemTime::now().duration_since(UNIX_EPOCH)?;

        let chain_finality = ctx.chain_config().policy.chain_finality;

        node_status.sync_status.epoch = head.epoch() as u64;
        node_status.sync_status.behind =
            epochs_behind_head(head.min_timestamp(), cur_duration.as_secs())?;

        if incl_chain_status && head.epoch() > chain_finality {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epochs_behind_head_is_reported_in_epochs() {
        let epoch = EPOCH_DURATION_SECONDS as u64;
        assert_eq!(epochs_behind_head(1_000, 1_000).unwrap(), 0);
        assert_eq!(epochs_behind_head(1_000, 1_000 + epoch).unwrap(), 1);
        assert_eq!(epochs_behind_head(1_000, 1_000 + 10 * epoch).unwrap(), 10);
        assert_eq!(epochs_behind_head(1_000, 1_000 + epoch - 1).unwrap(), 0);
        assert_eq!(epochs_behind_head(1_000, 1_000 + 2 * epoch - 1).unwrap(), 1);
    }

    #[test]
    fn epochs_behind_head_tolerates_skew_up_to_one_epoch() {
        let epoch = EPOCH_DURATION_SECONDS as u64;
        assert_eq!(epochs_behind_head(1_001, 1_000).unwrap(), 0);
        assert_eq!(epochs_behind_head(1_000 + epoch, 1_000).unwrap(), 0);
    }

    #[test]
    fn epochs_behind_head_errors_on_skew_above_one_epoch() {
        let epoch = EPOCH_DURATION_SECONDS as u64;
        assert!(epochs_behind_head(1_000 + epoch + 1, 1_000).is_err());
    }
}
