// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::blocks::{BlockHeader, Tipset, TipsetKeys};
use crate::chain::index::ResolveNullTipset;
use crate::cid_collections::CidHashSet;
use crate::lotus_json::LotusJson;
use crate::message::ChainMessage;
use crate::rpc_api::data_types::{ApiMessage, ApiReceipt};
use crate::rpc_api::{
    chain_api::*,
    data_types::{BlockMessages, RPCState},
};
use crate::shim::clock::ChainEpoch;
use crate::shim::message::Message;
use crate::utils::io::VoidAsyncWriter;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_shared4::receipt::Receipt;
use hex::ToHex;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use once_cell::sync::Lazy;
use sha2::Sha256;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(in crate::rpc) async fn chain_get_message<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((msg_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Message>, JsonRpcError> {
    let chain_message: ChainMessage = data
        .state_manager
        .blockstore()
        .get_cbor(&msg_cid)?
        .ok_or_else(|| format!("can't find message with cid {msg_cid}"))?;
    Ok(LotusJson(match chain_message {
        ChainMessage::Signed(m) => m.into_message(),
        ChainMessage::Unsigned(m) => m,
    }))
}

pub(in crate::rpc) async fn chain_get_parent_message<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((block_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Vec<ApiMessage>>, JsonRpcError> {
    let store = data.state_manager.blockstore();
    let block_header: BlockHeader = store
        .get_cbor(&block_cid)?
        .ok_or_else(|| format!("can't find block header with cid {block_cid}"))?;
    if block_header.epoch() == 0 {
        Ok(LotusJson(vec![]))
    } else {
        let parent_tipset = Tipset::load_required(store, block_header.parents())?;
        let messages = load_api_messages_from_tipset(store, &parent_tipset)?;
        Ok(LotusJson(messages))
    }
}

pub(in crate::rpc) async fn chain_get_parent_receipts<DB: Blockstore + Send + Sync + 'static>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((block_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Vec<ApiReceipt>>, JsonRpcError> {
    let store = data.state_manager.blockstore();
    let block_header: BlockHeader = store
        .get_cbor(&block_cid)?
        .ok_or_else(|| format!("can't find block header with cid {block_cid}"))?;
    let mut receipts = Vec::new();
    if block_header.epoch() == 0 {
        return Ok(LotusJson(vec![]));
    }
    let amt = Amt::<Receipt, _>::load(block_header.message_receipts(), store).map_err(|_| {
        JsonRpcError::Full {
            code: 1,
            message: format!(
                "failed to root: ipld: could not find {}",
                block_header.message_receipts()
            ),
            data: None,
        }
    })?;

    amt.for_each(|_, receipt| {
        receipts.push(ApiReceipt {
            exit_code: receipt.exit_code.into(),
            return_data: receipt.return_data.clone(),
            gas_used: receipt.gas_used,
            events_root: receipt.events_root,
        });
        Ok(())
    })?;

    Ok(LotusJson(receipts))
}

pub(crate) async fn chain_get_messages_in_tipset<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((tsk,))): Params<LotusJson<(TipsetKeys,)>>,
) -> Result<LotusJson<Vec<ApiMessage>>, JsonRpcError> {
    let store = data.chain_store.blockstore();
    let tipset = Tipset::load_required(store, &tsk)?;
    let messages = load_api_messages_from_tipset(store, &tipset)?;
    Ok(LotusJson(messages))
}

pub(in crate::rpc) async fn chain_export<DB>(
    data: Data<RPCState<DB>>,
    Params(ChainExportParams {
        epoch,
        recent_roots,
        output_path,
        tipset_keys: tsk,
        skip_checksum,
        dry_run,
    }): Params<ChainExportParams>,
) -> Result<Option<String>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    let _locked = LOCK.try_lock();
    if _locked.is_err() {
        return Err(JsonRpcError::Provided {
            code: http::StatusCode::SERVICE_UNAVAILABLE.as_u16() as _,
            message: "Another chain export job is still in progress",
        });
    }

    let chain_finality = data.state_manager.chain_config().policy.chain_finality;
    if recent_roots < chain_finality {
        Err(&format!(
            "recent-stateroots must be greater than {chain_finality}"
        ))?;
    }

    let head = data.chain_store.load_required_tipset(&tsk)?;
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
        Err(e) => Err(JsonRpcError::from(e)),
    }
}

pub(in crate::rpc) async fn chain_read_obj<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((obj_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Vec<u8>>, JsonRpcError> {
    let bytes = data
        .state_manager
        .blockstore()
        .get(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(LotusJson(bytes))
}

pub(in crate::rpc) async fn chain_has_obj<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((obj_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<bool, JsonRpcError> {
    Ok(data.state_manager.blockstore().get(&obj_cid)?.is_some())
}

pub(in crate::rpc) async fn chain_get_block_messages<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((blk_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<BlockMessages, JsonRpcError> {
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .ok_or("can't find block with that cid")?;
    let blk_msgs = blk.messages();
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

pub(in crate::rpc) async fn chain_get_tipset_by_height<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((height, tsk))): Params<LotusJson<(ChainEpoch, TipsetKeys)>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset(&tsk)?;
    let tss = data
        .state_manager
        .chain_store()
        .chain_index
        .tipset_by_height(height, ts, ResolveNullTipset::TakeOlder)?;
    Ok((*tss).clone().into())
}

pub(in crate::rpc) async fn chain_get_genesis<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<Option<LotusJson<Tipset>>, JsonRpcError> {
    let genesis = data.state_manager.chain_store().genesis();
    Ok(Some(Tipset::from(genesis).into()))
}

pub(in crate::rpc) async fn chain_head<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let heaviest = data.state_manager.chain_store().heaviest_tipset();
    Ok((*heaviest).clone().into())
}

pub(in crate::rpc) async fn chain_get_block<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((blk_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<BlockHeader>, JsonRpcError> {
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .ok_or("can't find BlockHeader with that cid")?;
    Ok(blk.into())
}

pub(in crate::rpc) async fn chain_get_tipset<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((tsk,))): Params<LotusJson<(TipsetKeys,)>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset(&tsk)?;
    Ok((*ts).clone().into())
}

// This is basically a port of the reference implementation at
// https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321
pub(in crate::rpc) async fn chain_set_head<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((tsk,))): Params<LotusJson<(TipsetKeys,)>>,
) -> Result<(), JsonRpcError> {
    let new_head = data
        .state_manager
        .chain_store()
        .load_required_tipset(&tsk)?;
    let mut current = data.state_manager.chain_store().heaviest_tipset();
    while current.epoch() >= new_head.epoch() {
        for cid in current.key().cids.clone() {
            data.state_manager
                .chain_store()
                .unmark_block_as_validated(&cid);
        }
        let parents = current.blocks()[0].parents();
        current = data
            .state_manager
            .chain_store()
            .load_required_tipset(parents)?;
    }
    data.state_manager
        .chain_store()
        .set_heaviest_tipset(new_head)
        .map_err(Into::into)
}

pub(crate) async fn chain_get_min_base_fee<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params((basefee_lookback,)): Params<(u32,)>,
) -> Result<String, JsonRpcError> {
    let mut current = data.state_manager.chain_store().heaviest_tipset();
    let mut min_base_fee = current.blocks()[0].parent_base_fee().clone();

    for _ in 0..basefee_lookback {
        let parents = current.blocks()[0].parents();
        current = data
            .state_manager
            .chain_store()
            .load_required_tipset(parents)?;

        min_base_fee = min_base_fee.min(current.blocks()[0].parent_base_fee().to_owned());
    }

    Ok(min_base_fee.atto().to_string())
}

fn load_api_messages_from_tipset(
    store: &impl Blockstore,
    tipset: &Tipset,
) -> Result<Vec<ApiMessage>, JsonRpcError> {
    let full_tipset = tipset
        .fill_from_blockstore(store)
        .ok_or_else(|| anyhow::anyhow!("Failed to load full tipset"))?;
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
