// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::State;
use blocks::BlockHeader;
use blockstore::BlockStore;
use cid::json::CidJson;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{
    signed_message::json::SignedMessageJson, unsigned_message::json::UnsignedMessageJson,
    UnsignedMessage,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct BlockMessage {
    #[serde(rename = "BlsMessages")]
    pub bls_msg: Option<Vec<UnsignedMessageJson>>,
    #[serde(rename = "SecpkMessages")]
    pub secp_msg: Option<Vec<SignedMessageJson>>,
    #[serde(rename = "Cids")]
    pub cids: Option<Vec<CidJson>>,
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
) -> Result<BlockMessage, JsonRpcError> {
    let blk_cid = (params.0).0;
    let blk: BlockHeader = data
        .store
        .get(&blk_cid)?
        .ok_or("can't find block with that cid")?;

    let (unsigned, signed) = chain::messages(data.store.as_ref(), &blk)?;
    let (unsigned_cid, signed_cid) = chain::read_msg_cids(data.store.as_ref(), blk.messages())?;
    let cids = unsigned_cid
        .into_iter()
        .chain(signed_cid)
        .collect::<Vec<_>>();

    let ret = BlockMessage {
        bls_msg: if unsigned.is_empty() {
            None
        } else {
            Some(unsigned.into_iter().map(UnsignedMessageJson).collect())
        },
        secp_msg: if signed.is_empty() {
            None
        } else {
            Some(signed.into_iter().map(SignedMessageJson).collect())
        },
        cids: if cids.is_empty() {
            None
        } else {
            Some(cids.into_iter().map(CidJson).collect())
        },
    };
    Ok(ret)
}
