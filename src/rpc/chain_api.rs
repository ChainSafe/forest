// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::sync::Arc;

use crate::blocks::{
    header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
    BlockHeader, Tipset,
};
use crate::chain::index::ResolveNullTipset;
use crate::ipld::CidHashSet;
use crate::json::{cid::CidJson, message::json::MessageJson};
use crate::rpc_api::{
    chain_api::*,
    data_types::{BlockMessages, RPCState},
};
use crate::shim::message::Message;
use crate::utils::io::VoidAsyncWriter;
use anyhow::Result;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use hex::ToHex;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use sha2::Sha256;
use tokio::sync::Mutex;

pub(in crate::rpc) async fn chain_get_message<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainGetMessageParams>,
) -> Result<ChainGetMessageResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (CidJson(msg_cid),) = params;
    let ret: Message = data
        .state_manager
        .blockstore()
        .get_cbor(&msg_cid)?
        .ok_or("can't find message with that cid")?;
    Ok(MessageJson(ret))
}

pub(in crate::rpc) async fn chain_export<DB>(
    data: Data<RPCState<DB>>,
    Params(ChainExportParams {
        epoch,
        recent_roots,
        output_path,
        tipset_keys: TipsetKeysJson(tsk),
        skip_checksum,
        dry_run,
    }): Params<ChainExportParams>,
) -> Result<ChainExportResult, JsonRpcError>
where
    DB: Blockstore,
{
    lazy_static::lazy_static! {
        static ref LOCK: Mutex<()> = Mutex::new(());
    }

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

    let head = data.chain_store.tipset_from_keys(&tsk)?;
    let start_ts =
        data.chain_store
            .chain_index
            .tipset_by_height(epoch, head, ResolveNullTipset::TakeOlder)?;

    match if dry_run {
        crate::chain::export::<Sha256>(
            &data.chain_store.db,
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
            &data.chain_store.db,
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

pub(in crate::rpc) async fn chain_read_obj<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainReadObjParams>,
) -> Result<ChainReadObjResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (CidJson(obj_cid),) = params;
    let ret = data
        .state_manager
        .blockstore()
        .get(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(hex::encode(ret))
}

pub(in crate::rpc) async fn chain_has_obj<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainHasObjParams>,
) -> Result<ChainHasObjResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (CidJson(obj_cid),) = params;
    Ok(data.state_manager.blockstore().get(&obj_cid)?.is_some())
}

pub(in crate::rpc) async fn chain_get_block_messages<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainGetBlockMessagesParams>,
) -> Result<ChainGetBlockMessagesResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (CidJson(blk_cid),) = params;
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

pub(in crate::rpc) async fn chain_get_tipset_by_height<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainGetTipsetByHeightParams>,
) -> Result<ChainGetTipsetByHeightResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (height, tsk) = params;
    let ts = data.state_manager.chain_store().tipset_from_keys(&tsk)?;
    let tss = data
        .state_manager
        .chain_store()
        .chain_index
        .tipset_by_height(height, ts, ResolveNullTipset::TakeOlder)?;
    Ok(TipsetJson(tss))
}

pub(in crate::rpc) async fn chain_get_genesis<DB>(
    data: Data<RPCState<DB>>,
) -> Result<ChainGetGenesisResult, JsonRpcError>
where
    DB: Blockstore,
{
    let genesis = data.state_manager.chain_store().genesis();
    let gen_ts = Arc::new(Tipset::from(genesis));
    Ok(Some(TipsetJson(gen_ts)))
}

pub(in crate::rpc) async fn chain_head<DB>(
    data: Data<RPCState<DB>>,
) -> Result<ChainHeadResult, JsonRpcError>
where
    DB: Blockstore,
{
    let heaviest = data.state_manager.chain_store().heaviest_tipset();
    Ok(TipsetJson(heaviest))
}

pub(in crate::rpc) async fn chain_get_block<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainGetBlockParams>,
) -> Result<ChainGetBlockResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (CidJson(blk_cid),) = params;
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .ok_or("can't find BlockHeader with that cid")?;
    Ok(BlockHeaderJson(blk))
}

pub(in crate::rpc) async fn chain_get_tipset<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainGetTipSetParams>,
) -> Result<ChainGetTipSetResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.state_manager.chain_store().tipset_from_keys(&tsk)?;
    Ok(TipsetJson(ts))
}

pub(in crate::rpc) async fn chain_get_name<DB>(
    data: Data<RPCState<DB>>,
) -> Result<ChainGetNameResult, JsonRpcError>
where
    DB: Blockstore,
{
    Ok(data.state_manager.chain_config().network.to_string())
}

// This is basically a port of the reference implementation at
// https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321
pub(in crate::rpc) async fn chain_set_head<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<ChainSetHeadParams>,
) -> Result<ChainSetHeadResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (params,) = params;
    let new_head = data.state_manager.chain_store().tipset_from_keys(&params)?;
    let mut current = data.state_manager.chain_store().heaviest_tipset();
    while current.epoch() >= new_head.epoch() {
        for cid in current.key().cids() {
            data.state_manager
                .chain_store()
                .unmark_block_as_validated(cid);
        }
        let parents = current.blocks()[0].parents();
        current = data.state_manager.chain_store().tipset_from_keys(parents)?;
    }
    data.state_manager
        .chain_store()
        .set_heaviest_tipset(new_head)
        .map_err(Into::into)
}
