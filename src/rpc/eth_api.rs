// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::eth::{Address, BlockNumberOrHash};
use crate::lotus_json::LotusJson;
use crate::rpc_api::data_types::RPCState;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

pub(in crate::rpc) async fn eth_chain_id<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<String, JsonRpcError> {
    Ok(format!(
        "0x{:x}",
        data.state_manager.chain_config().eth_chain_id
    ))
}

//     Params(LotusJson((address, tipset_keys))): Params<LotusJson<(Address, TipsetKeys)>>,
pub(in crate::rpc) async fn eth_get_balance<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((address, block_number))): Params<LotusJson<(Address, BlockNumberOrHash)>>,
) -> Result<String, JsonRpcError> {
    // TODO
    Ok("0".to_string())
}
