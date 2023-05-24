// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forest_beacon::Beacon;
use forest_blocks::{tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson};
use forest_rpc_api::{
    data_types::{node_api::NodeStatusInfo, RPCState},
    node_api::NodeStatusResult,
};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

use crate::wallet_api::{wallet_balance, wallet_default_address};

pub(crate) async fn node_status<DB: Blockstore + Clone + Send + Sync + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
) -> Result<NodeStatusResult, JsonRpcError> {
    let chain_finality = data.state_manager.chain_config().policy.chain_finality;
    let mut ts = data.state_manager.chain_store().heaviest_tipset();
    let mut tipsets = Vec::with_capacity(chain_finality as usize);
    for _ in 0..(chain_finality - 1).min(ts.epoch()) {
        let parent_tipset_keys = TipsetKeysJson(ts.parents().clone());
        let tsjson = data
            .state_manager
            .chain_store()
            .tipset_from_keys(&parent_tipset_keys.0)?;
        tipsets.push(TipsetJson(tsjson.clone()));
        ts = tsjson;
    }

    let cur_duration: Duration = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let mut node_status = NodeStatusInfo::new(cur_duration, tipsets, chain_finality as usize)?;

    node_status.start_time = data.start_time;
    node_status.network = data.network_name.to_string();
    let default_wallet_address = wallet_default_address(Data(data.0.clone())).await?;
    node_status.default_wallet_address = default_wallet_address.clone();
    let default_wallet_address_balance = if let Some(def_addr) = &default_wallet_address {
        let balance = wallet_balance(data, Params((def_addr.clone(),))).await?;
        Some(balance)
    } else {
        None
    };

    node_status.default_wallet_address_balance = default_wallet_address_balance;

    Ok(node_status)
}
