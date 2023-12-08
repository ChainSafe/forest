// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::{ops::Add, sync::Arc};

use super::gas_api;
use crate::blocks::{Tipset, TipsetKeys};
use crate::chain::{index::ResolveNullTipset, ChainStore};
use crate::cid_collections::FrozenCidVec;
use crate::eth::{BlockNumberOrHash, Predefined};
use crate::lotus_json::LotusJson;
use crate::rpc_api::{data_types::RPCState, eth_api::BigInt as EthBigInt, eth_api::*};
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};

use anyhow::{bail, Context};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use num_bigint::BigInt;
use num_traits::Zero as _;

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

pub(in crate::rpc) async fn eth_gas_price<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<GasPriceResult, JsonRpcError> {
    let ts = data.state_manager.chain_store().heaviest_tipset();
    let block0 = ts
        .blocks()
        .first()
        .context("Failed to get the first block")?;
    let base_fee = block0.parent_base_fee();
    if let Ok(premium) = gas_api::estimate_gas_premium(&data, 10000).await {
        let gas_price = base_fee.add(premium);
        Ok(GasPriceResult(gas_price.atto().clone()))
    } else {
        Ok(GasPriceResult(BigInt::zero()))
    }
}

pub(in crate::rpc) async fn eth_get_balance<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((address, block_param))): Params<LotusJson<(Address, BlockNumberOrHash)>>,
) -> Result<EthBigInt, JsonRpcError> {
    let fil_addr = address.to_filecoin_address()?;

    let ts = tipset_by_block_number_or_hash(&data.chain_store, block_param)?;

    let state = StateTree::new_from_root(data.state_manager.blockstore_owned(), ts.parent_state())?;

    let actor = state
        .get_actor(&fil_addr)
        .map_err(|_e| JsonRpcError::Provided {
            code: http::StatusCode::SERVICE_UNAVAILABLE.as_u16() as _,
            message: "Failed to retrieve actor",
        })?
        .ok_or(JsonRpcError::INTERNAL_ERROR)?;

    Ok(EthBigInt(actor.balance.atto().clone()))
}

fn tipset_by_block_number_or_hash<DB: Blockstore>(
    chain: &Arc<ChainStore<DB>>,
    block_param: BlockNumberOrHash,
) -> anyhow::Result<Arc<Tipset>> {
    let head = chain.heaviest_tipset();

    match block_param {
        BlockNumberOrHash::PredefinedBlock(predefined) => match predefined {
            Predefined::Earliest => bail!("block param \"earliest\" is not supported"),
            Predefined::Pending => Ok(head),
            Predefined::Latest => {
                let parent = chain.chain_index.load_required_tipset(head.parents())?;
                Ok(parent)
            }
        },
        BlockNumberOrHash::BlockNumber(number) => {
            let height = ChainEpoch::from(number as i64); // TODO: check conversion
            if height > head.epoch() - 1 {
                bail!("requested a future epoch (beyond \"latest\")");
            }
            let ts =
                chain
                    .chain_index
                    .tipset_by_height(height, head, ResolveNullTipset::TakeOlder)?;
            Ok(ts)
        }
        BlockNumberOrHash::BlockHash(hash, require_canonical) => {
            let tsk = TipsetKeys {
                cids: FrozenCidVec::from_iter([hash.to_cid()]),
            };
            let ts = chain.chain_index.load_required_tipset(&tsk)?;
            // verify that the tipset is in the canonical chain
            if require_canonical {
                // walk up the current chain (our head) until we reach ts.epoch()
                let walk_ts = chain.chain_index.tipset_by_height(
                    ts.epoch(),
                    head,
                    ResolveNullTipset::TakeOlder,
                )?;
                // verify that it equals the expected tipset
                if walk_ts != ts {
                    bail!("tipset is not canonical");
                }
            }
            Ok(ts)
        }
    }
}
