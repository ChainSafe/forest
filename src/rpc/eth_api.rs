// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::rpc_api::data_types::RPCState;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError};

// EthAccounts will always return [] since we don't expect Forest to manage private keys
pub(in crate::rpc) async fn eth_accounts() -> Result<Vec<String>, JsonRpcError> {
    Ok(vec![])
}

// `eth_block_number` needs to return the height of the latest committed tipset.
// Ethereum clients expect all transactions included in this block to have execution outputs.
// This is the parent of the head tipset. The head tipset is speculative, has not been
// recognized by the network, and its messages are only included, not executed.
// See https://github.com/filecoin-project/ref-fvm/issues/1135.
pub(in crate::rpc) async fn eth_block_number<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<String, JsonRpcError> {
    let heaviest = data.state_manager.chain_store().heaviest_tipset();
    if heaviest.epoch() == 0 {
        // We're at genesis.
        return Ok("0x0".to_string());
    }
    // First non-null parent.
    let effective_parent = heaviest.parents();
    let parent = data
        .state_manager
        .chain_store()
        .load_tipset(effective_parent);
    match parent {
        Ok(parent) => match parent {
            Some(parent) => Ok(format!("0x{:x}", parent.epoch())),
            None => Ok("0x0".to_string()),
        },
        Err(_) => Ok("0x0".to_string()),
    }
}

pub(in crate::rpc) async fn eth_chain_id<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<String, JsonRpcError> {
    Ok(format!(
        "0x{:x}",
        data.state_manager.chain_config().eth_chain_id
    ))
}
