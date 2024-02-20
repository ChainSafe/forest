// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::index::ResolveNullTipset;
use crate::cid_collections::CidHashSet;
use crate::lotus_json::LotusJson;
use crate::message::ChainMessage;
use crate::rpc::error::JsonRpcError;
use crate::rpc_api::data_types::{ApiMessage, ApiReceipt};
use crate::rpc_api::{
    chain_api::*,
    data_types::{ApiTipsetKey, BlockMessages, Data, RPCState},
};
use crate::shim::clock::ChainEpoch;
use crate::shim::message::Message;
use crate::utils::io::VoidAsyncWriter;
use anyhow::{Context, Result};
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use hex::ToHex;
use jsonrpsee::types::error::ErrorObjectOwned;
use jsonrpsee::types::Params;
use once_cell::sync::Lazy;
use sha2::Sha256;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn chain_get_message<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Message>, JsonRpcError> {
    let LotusJson((msg_cid,)): LotusJson<(Cid,)> = params.parse()?;

    let chain_message: ChainMessage = data
        .state_manager
        .blockstore()
        .get_cbor(&msg_cid)?
        .with_context(|| format!("can't find message with cid {msg_cid}"))?;
    Ok(LotusJson(match chain_message {
        ChainMessage::Signed(m) => m.into_message(),
        ChainMessage::Unsigned(m) => m,
    }))
}

pub async fn chain_get_parent_messages<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Vec<ApiMessage>>, JsonRpcError> {
    let LotusJson((block_cid,)): LotusJson<(Cid,)> = params.parse()?;

    let store = data.state_manager.blockstore();
    let block_header: CachingBlockHeader = store
        .get_cbor(&block_cid)?
        .with_context(|| format!("can't find block header with cid {block_cid}"))?;
    if block_header.epoch == 0 {
        Ok(LotusJson(vec![]))
    } else {
        let parent_tipset = Tipset::load_required(store, &block_header.parents)?;
        let messages = load_api_messages_from_tipset(store, &parent_tipset)?;
        Ok(LotusJson(messages))
    }
}

pub async fn chain_get_parent_receipts<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Vec<ApiReceipt>>, JsonRpcError> {
    let LotusJson((block_cid,)): LotusJson<(Cid,)> = params.parse()?;

    let store = data.state_manager.blockstore();
    let block_header: CachingBlockHeader = store
        .get_cbor(&block_cid)?
        .with_context(|| format!("can't find block header with cid {block_cid}"))?;
    let mut receipts = Vec::new();
    if block_header.epoch == 0 {
        return Ok(LotusJson(vec![]));
    }

    // Try Receipt_v4 first. (Receipt_v4 and Receipt_v3 are identical, use v4 here)
    if let Ok(amt) =
        Amt::<fvm_shared4::receipt::Receipt, _>::load(&block_header.message_receipts, store)
            .map_err(|_| {
                ErrorObjectOwned::owned::<()>(
                    1,
                    format!(
                        "failed to root: ipld: could not find {}",
                        block_header.message_receipts
                    ),
                    None,
                )
            })
    {
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: receipt.return_data.clone(),
                gas_used: receipt.gas_used,
                events_root: receipt.events_root,
            });
            Ok(())
        })?;
    } else {
        // Fallback to Receipt_v2.
        let amt =
            Amt::<fvm_shared2::receipt::Receipt, _>::load(&block_header.message_receipts, store)
                .map_err(|_| {
                    ErrorObjectOwned::owned::<()>(
                        1,
                        format!(
                            "failed to root: ipld: could not find {}",
                            block_header.message_receipts
                        ),
                        None,
                    )
                })?;
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: receipt.return_data.clone(),
                gas_used: receipt.gas_used as _,
                events_root: None,
            });
            Ok(())
        })?;
    }

    Ok(LotusJson(receipts))
}

pub(crate) async fn chain_get_messages_in_tipset<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Vec<ApiMessage>>, JsonRpcError> {
    let LotusJson((tsk,)): LotusJson<(TipsetKey,)> = params.parse()?;

    let store = data.chain_store.blockstore();
    let tipset = Tipset::load_required(store, &tsk)?;
    let messages = load_api_messages_from_tipset(store, &tipset)?;
    Ok(LotusJson(messages))
}

pub async fn chain_export<DB>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<Option<String>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let ChainExportParams {
        epoch,
        recent_roots,
        output_path,
        tipset_keys: ApiTipsetKey(tsk),
        skip_checksum,
        dry_run,
    }: ChainExportParams = params.parse()?;

    static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    let _locked = LOCK.try_lock();
    if _locked.is_err() {
        return Err(anyhow::anyhow!("Another chain export job is still in progress").into());
    }

    let chain_finality = data.state_manager.chain_config().policy.chain_finality;
    if recent_roots < chain_finality {
        return Err(anyhow::anyhow!(format!(
            "recent-stateroots must be greater than {chain_finality}"
        ))
        .into());
    }

    let head = data.chain_store.load_required_tipset_with_fallback(&tsk)?;
    let start_ts =
        data.chain_store
            .chain_index
            .tipset_by_height(epoch, head, ResolveNullTipset::TakeOlder)?;

    match if dry_run {
        crate::chain::export::<Sha256>(
            Arc::clone(&data.chain_store.db),
            &start_ts,
            recent_roots,
            VoidAsyncWriter,
            CidHashSet::default(),
            skip_checksum,
        )
        .await
    } else {
        let file = tokio::fs::File::create(&output_path).await?;
        crate::chain::export::<Sha256>(
            Arc::clone(&data.chain_store.db),
            &start_ts,
            recent_roots,
            file,
            CidHashSet::default(),
            skip_checksum,
        )
        .await
    } {
        Ok(checksum_opt) => Ok(checksum_opt.map(|hash| hash.encode_hex())),
        Err(e) => Err(anyhow::anyhow!(e).into()),
    }
}

pub async fn chain_read_obj<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Vec<u8>>, JsonRpcError> {
    let LotusJson((obj_cid,)): LotusJson<(Cid,)> = params.parse()?;

    let bytes = data
        .state_manager
        .blockstore()
        .get(&obj_cid)?
        .context("can't find object with that cid")?;
    Ok(LotusJson(bytes))
}

pub async fn chain_has_obj<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<bool, JsonRpcError> {
    let LotusJson((obj_cid,)): LotusJson<(Cid,)> = params.parse()?;

    Ok(data.state_manager.blockstore().get(&obj_cid)?.is_some())
}

pub async fn chain_get_block_messages<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<BlockMessages, JsonRpcError> {
    let LotusJson((blk_cid,)): LotusJson<(Cid,)> = params.parse()?;

    let blk: CachingBlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .context("can't find block with that cid")?;
    let blk_msgs = &blk.messages;
    let (unsigned_cids, signed_cids) =
        crate::chain::read_msg_cids(data.state_manager.blockstore(), blk_msgs)?;
    let (bls_msg, secp_msg) = crate::chain::block_messages_from_cids(
        data.state_manager.blockstore(),
        &unsigned_cids,
        &signed_cids,
    )?;
    let cids = unsigned_cids
        .into_iter()
        .chain(signed_cids)
        .collect::<Vec<_>>();

    let ret = BlockMessages {
        bls_msg,
        secp_msg,
        cids,
    };
    Ok(ret)
}

pub async fn chain_get_tipset_by_height<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let LotusJson((height, ApiTipsetKey(tsk))): LotusJson<(ChainEpoch, ApiTipsetKey)> =
        params.parse()?;

    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_with_fallback(&tsk)?;
    let tss = data
        .state_manager
        .chain_store()
        .chain_index
        .tipset_by_height(height, ts, ResolveNullTipset::TakeOlder)?;
    Ok((*tss).clone().into())
}

pub async fn chain_get_genesis<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<Option<LotusJson<Tipset>>, JsonRpcError> {
    let genesis = data.state_manager.chain_store().genesis_block_header();
    Ok(Some(Tipset::from(genesis).into()))
}

pub async fn chain_head<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let heaviest = data.state_manager.chain_store().heaviest_tipset();
    Ok((*heaviest).clone().into())
}

pub async fn chain_get_block<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<CachingBlockHeader>, JsonRpcError> {
    let LotusJson((blk_cid,)): LotusJson<(Cid,)> = params.parse()?;

    let blk: CachingBlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .context("can't find BlockHeader with that cid")?;
    Ok(blk.into())
}

pub async fn chain_get_tipset<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let LotusJson((ApiTipsetKey(tsk),)): LotusJson<(ApiTipsetKey,)> = params.parse()?;

    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_with_fallback(&tsk)?;
    Ok((*ts).clone().into())
}

// This is basically a port of the reference implementation at
// https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321
pub async fn chain_set_head<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<(), JsonRpcError> {
    let LotusJson((ApiTipsetKey(tsk),)): LotusJson<(ApiTipsetKey,)> = params.parse()?;

    let new_head = data
        .state_manager
        .chain_store()
        .load_required_tipset_with_fallback(&tsk)?;
    let mut current = data.state_manager.chain_store().heaviest_tipset();
    while current.epoch() >= new_head.epoch() {
        for cid in current.key().cids.clone() {
            data.state_manager
                .chain_store()
                .unmark_block_as_validated(&cid);
        }
        let parents = &current.block_headers().first().parents;
        current = data
            .state_manager
            .chain_store()
            .chain_index
            .load_required_tipset(parents)?;
    }
    data.state_manager
        .chain_store()
        .set_heaviest_tipset(new_head)
        .map_err(Into::into)
}

pub(crate) async fn chain_get_min_base_fee<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<String, JsonRpcError> {
    let (basefee_lookback,): (u32,) = params.parse()?;

    let mut current = data.state_manager.chain_store().heaviest_tipset();
    let mut min_base_fee = current.block_headers().first().parent_base_fee.clone();

    for _ in 0..basefee_lookback {
        let parents = &current.block_headers().first().parents;
        current = data
            .state_manager
            .chain_store()
            .chain_index
            .load_required_tipset(parents)?;

        min_base_fee = min_base_fee.min(current.block_headers().first().parent_base_fee.to_owned());
    }

    Ok(min_base_fee.atto().to_string())
}

fn load_api_messages_from_tipset(
    store: &impl Blockstore,
    tipset: &Tipset,
) -> Result<Vec<ApiMessage>, JsonRpcError> {
    let full_tipset = tipset
        .fill_from_blockstore(store)
        .context("Failed to load full tipset")?;
    let blocks = full_tipset.into_blocks();
    let mut messages = vec![];
    let mut seen = CidHashSet::default();
    for block in blocks {
        for msg in block.bls_msgs() {
            let cid = msg.cid()?;
            if seen.insert(cid) {
                messages.push(ApiMessage::new(cid, msg.clone()));
            }
        }

        for msg in block.secp_msgs() {
            let cid = msg.cid()?;
            if seen.insert(cid) {
                messages.push(ApiMessage::new(cid, msg.message.clone()));
            }
        }
    }

    Ok(messages)
}
