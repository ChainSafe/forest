// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::State;
use blocks::{tipset_json::TipsetJson, BlockHeader, Tipset, TipsetKeys};
use blockstore::BlockStore;
use cid::{json::CidJson, Cid};
use clock::ChainEpoch;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{
    signed_message,
    unsigned_message::{self, json::UnsignedMessageJson},
    SignedMessage, UnsignedMessage,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct BlockMessages {
    #[serde(rename = "BlsMessages", with = "unsigned_message::json::vec")]
    pub bls_msg: Vec<UnsignedMessage>,
    #[serde(rename = "SecpkMessages", with = "signed_message::json::vec")]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "cid::json::vec")]
    pub cids: Vec<Cid>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Message {
    #[serde(with = "cid::json")]
    cid: Cid,
    #[serde(with = "unsigned_message::json")]
    message: UnsignedMessage,
}

pub(crate) async fn chain_get_message<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<UnsignedMessageJson, JsonRpcError> {
    let msg_cid = (params.0).0;
    let ret: UnsignedMessage = data
        .store
        .get(&msg_cid)?
        .ok_or("can't find message with that cid")?;
    Ok(UnsignedMessageJson(ret))
}

pub(crate) async fn chain_read_obj<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<Vec<u8>, JsonRpcError> {
    let obj_cid = (params.0).0;
    let ret = data
        .store
        .get_bytes(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(ret)
}

pub(crate) async fn chain_has_obj<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<bool, JsonRpcError> {
    let obj_cid = (params.0).0;
    Ok(data.store.get_bytes(&obj_cid)?.is_some())
}
pub(crate) async fn chain_block_messages<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<BlockMessages, JsonRpcError> {
    let blk_cid = (params.0).0;
    let blk: BlockHeader = data
        .store
        .get(&blk_cid)?
        .ok_or("can't find block with that cid")?;
    let blk_msgs = blk.messages();
    let (unsigned_cids, signed_cids) = chain::read_msg_cids(data.store.as_ref(), &blk_msgs)?;
    let (bls_msg, secp_msg) =
        chain::block_messages_from_cids(data.store.as_ref(), &unsigned_cids, &signed_cids)?;
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

pub(crate) async fn chain_get_tipset_by_height<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(ChainEpoch, TipsetKeys)>,
) -> Result<TipsetJson, JsonRpcError> {
    let height = params.0;
    let tsk = params.1;
    let ts = chain::tipset_from_keys(data.store.as_ref(), &tsk).unwrap();
    let tss = chain::tipset_by_height(data.store.as_ref(), height, ts, true)?;
    Ok(TipsetJson(tss))
}

pub(crate) async fn chain_get_genesis<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
) -> Result<Option<TipsetJson>, JsonRpcError> {
    let genesis = chain::genesis(data.store.as_ref())?;
    if genesis.is_none() {
        return Ok(None);
    }
    let genesis = genesis.unwrap();
    let gen_ts = Tipset::new(vec![genesis])?;
    Ok(Some(TipsetJson(gen_ts)))
}