// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::rpc_api::data_types::RPCState;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError};

// EthAccounts will always return [] since we don't expect Forest to manage private keys
pub(in crate::rpc) async fn eth_accounts<DB: Blockstore>() -> Result<String, JsonRpcError> {
    Ok("[]".to_string())
}

pub(in crate::rpc) async fn eth_chain_id<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<String, JsonRpcError> {
    Ok(format!(
        "0x{:x}",
        data.state_manager.chain_config().eth_chain_id
    ))
}
