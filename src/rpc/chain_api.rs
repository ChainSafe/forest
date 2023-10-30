// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::sync::Arc;

use crate::blocks::{BlockHeader, Tipset, TipsetKeys};
use crate::chain::index::ResolveNullTipset;
use crate::cid_collections::CidHashSet;
use crate::lotus_json::LotusJson;
use crate::rpc_api::{
    chain_api::*,
    data_types::{BlockMessages, RPCState},
};
use crate::shim::clock::ChainEpoch;
use crate::shim::message::Message;
use crate::utils::io::VoidAsyncWriter;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use hex::ToHex;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use once_cell::sync::Lazy;
use sha2::Sha256;
use tokio::sync::Mutex;

pub(in crate::rpc) async fn chain_get_message<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((msg_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Message>, JsonRpcError> {
    let ret: Message = data
        .state_manager
        .blockstore()
        .get_cbor(&msg_cid)?
        .ok_or("can't find message with that cid")?;
    Ok(LotusJson(ret))
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
) -> Result<String, JsonRpcError> {
    let ret = data
        .state_manager
        .blockstore()
        .get(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(hex::encode(ret))
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
