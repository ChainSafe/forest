// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::{ops::Add, sync::Arc};

use super::gas_api;
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{index::ResolveNullTipset, ChainStore};
use crate::chain_sync::SyncStage;
use crate::lotus_json::LotusJson;
use crate::rpc::error::JsonRpcError;
use crate::rpc::sync_api::sync_state;
use crate::rpc::Ctx;
use crate::rpc_api::data_types::RPCSyncState;
use crate::rpc_api::{eth_api::BigInt as EthBigInt, eth_api::*};
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};

use anyhow::{bail, Context, Result};
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use jsonrpsee::types::Params;
use nonempty::nonempty;
use num_bigint::BigInt;
use num_traits::Zero as _;

pub async fn eth_accounts() -> Result<Vec<String>, JsonRpcError> {
    // EthAccounts will always return [] since we don't expect Forest to manage private keys
    Ok(vec![])
}

pub async fn eth_block_number<DB: Blockstore>(data: Ctx<DB>) -> Result<String, JsonRpcError> {
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
    if let Ok(Some(parent)) = data
        .state_manager
        .chain_store()
        .chain_index
        .load_tipset(effective_parent)
    {
        Ok(format!("{:#x}", parent.epoch()))
    } else {
        Ok("0x0".to_string())
    }
}

pub async fn eth_chain_id<DB: Blockstore>(data: Ctx<DB>) -> Result<String, JsonRpcError> {
    Ok(format!(
        "{:#x}",
        data.state_manager.chain_config().eth_chain_id
    ))
}

pub async fn eth_gas_price<DB: Blockstore>(data: Ctx<DB>) -> Result<GasPriceResult, JsonRpcError> {
    let ts = data.state_manager.chain_store().heaviest_tipset();
    let block0 = ts.block_headers().first();
    let base_fee = &block0.parent_base_fee;
    if let Ok(premium) = gas_api::estimate_gas_premium(&data, 10000).await {
        let gas_price = base_fee.add(premium);
        Ok(GasPriceResult(gas_price.atto().clone()))
    } else {
        Ok(GasPriceResult(BigInt::zero()))
    }
}

pub async fn eth_get_balance<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<EthBigInt, JsonRpcError> {
    let LotusJson((address, block_param)): LotusJson<(Address, BlockNumberOrHash)> =
        params.parse()?;

    let fil_addr = address.to_filecoin_address()?;

    let ts = tipset_by_block_number_or_hash(&data.chain_store, block_param)?;

    let state = StateTree::new_from_root(data.state_manager.blockstore_owned(), ts.parent_state())?;

    let actor = state
        .get_actor(&fil_addr)?
        .context("Failed to retrieve actor")?;

    Ok(EthBigInt(actor.balance.atto().clone()))
}

pub async fn eth_syncing<DB: Blockstore>(
    _params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<EthSyncingResult>, JsonRpcError> {
    let RPCSyncState { active_syncs } = sync_state(data).await?;
    match active_syncs
        .iter()
        .rev()
        .find_or_first(|ss| ss.stage() != SyncStage::Idle)
    {
        Some(sync_state) => match (sync_state.base(), sync_state.target()) {
            (Some(base), Some(target)) => Ok(LotusJson(EthSyncingResult {
                done_sync: sync_state.stage() == SyncStage::Complete,
                current_block: sync_state.epoch(),
                starting_block: base.epoch(),
                highest_block: target.epoch(),
            })),
            _ => Err(JsonRpcError::internal_error(
                "missing syncing information, try again",
                None,
            )),
        },
        None => Err(JsonRpcError::internal_error("sync state not found", None)),
    }
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
            let height = ChainEpoch::from(number);
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
            let tsk = TipsetKey::from(nonempty![hash.to_cid()]);
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
