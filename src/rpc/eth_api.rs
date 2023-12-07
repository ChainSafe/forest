// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::sync::Arc;

use crate::blocks::Tipset;
use crate::chain::{index::ResolveNullTipset, ChainStore};
use crate::eth::{Address, BigInt, BlockNumberOrHash, Predefined};
use crate::lotus_json::LotusJson;
use crate::rpc_api::data_types::RPCState;
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};

use anyhow::bail;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

pub(in crate::rpc) async fn eth_accounts() -> Result<Vec<String>, JsonRpcError> {
    // EthAccounts will always return [] since we don't expect Forest to manage private keys
    Ok(vec![])
}

pub(in crate::rpc) async fn eth_block_number<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<String, JsonRpcError> {
    // `eth_block_number` needs to return the height of the latest committed tipset.
    // Ethereum clients expect all transactions included in this block to have execution outputs.
    // This is the parent of the head tipset. The head tipset is speculative, has not been
    // recognized by the network, and its messages are only included, not executed.
    // See https://github.com/filecoin-project/ref-fvm/issues/1135.
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

//     Params(LotusJson((address, tipset_keys))): Params<LotusJson<(Address, TipsetKeys)>>,
pub(in crate::rpc) async fn eth_get_balance<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((address, block_param))): Params<LotusJson<(Address, BlockNumberOrHash)>>,
) -> Result<LotusJson<BigInt>, JsonRpcError> {
    let fil_addr = address.to_filecoin_address()?;

    let ts = tipset_by_block_number_or_hash(&data.chain_store, block_param)?;

    let state = StateTree::new_from_root(data.state_manager.blockstore_owned(), ts.parent_state())?;

    let actor = state
        .get_actor(&fil_addr)
        .map_err(|e| JsonRpcError::Provided {
            code: http::StatusCode::SERVICE_UNAVAILABLE.as_u16() as _,
            message: "Failed to retrieve actor",
        })?
        .ok_or_else(|| JsonRpcError::INTERNAL_ERROR)?;

    let balance = BigInt(actor.balance.atto().clone());
    Ok(LotusJson(balance))
}

fn tipset_by_block_number_or_hash<DB: Blockstore>(
    chain: &Arc<ChainStore<DB>>,
    block_param: BlockNumberOrHash,
) -> anyhow::Result<Arc<Tipset>> {
    let head = chain.heaviest_tipset();

    if let Some(predefined) = block_param.predefined_block {
        match predefined {
            Predefined::Earliest => bail!("block param \"earliest\" is not supported"),
            Predefined::Pending => return Ok(head),
            Predefined::Latest => {
                let parent = chain.chain_index.load_required_tipset(head.parents())?;
                return Ok(parent);
            }
        }
    } else if let Some(block_number) = block_param.block_number {
        let height = ChainEpoch::from(block_number as i64); // TODO: check conversion
        if height > head.epoch() - 1 {
            bail!("requested a future epoch (beyond \"latest\")");
        }
        let ts = chain.chain_index.tipset_by_height(
            height as i64,
            head,
            ResolveNullTipset::TakeOlder,
        )?;
        return Ok(ts);
    } else if let Some(block_hash) = block_param.block_hash {
        //chain.chain_index.load_tipset();
        ()
    }

    bail!("invalid block param");
}
