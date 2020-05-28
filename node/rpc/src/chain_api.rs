// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::State;
use address::Address;
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
struct BlockMessage {
    #[serde(rename = "")]
    pub bls_msg: UnsignedMessageJson,
    pub secp_msg: SignedMessageJson,
    pub cids: Vec<CidJson>,
}

pub async fn chain_get_message<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<UnsignedMessageJson, JsonRpcError> {
    let msg_cid = (params.0).0;
    let ret: UnsignedMessage = data.store.get(&msg_cid).unwrap().unwrap();
    Ok(UnsignedMessageJson(ret))
}

pub async fn chain_read_obj<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<Vec<u8>, JsonRpcError> {
    let obj_cid = (params.0).0;
    let ret = data.store.get_bytes(&obj_cid).unwrap().unwrap();
    Ok(ret)
}

pub async fn chain_has_obj<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<bool, JsonRpcError> {
    let obj_cid = (params.0).0;
    let ret = data.store.get_bytes(&obj_cid).unwrap().is_some();
    Ok(ret)
}
async fn chain_block_messages<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<UnsignedMessageJson, JsonRpcError> {
    let blk_cid = (params.0).0;
    let blk: BlockHeader = data.store.get(&blk_cid).unwrap().unwrap();

    let (signed, unsigned) = chain::messages(data.store.as_ref(), &blk).unwrap();

    Ok(UnsignedMessageJson(ret))
}

//type BlockMessages struct {
//    BlsMessages   []*types.Message
//    SecpkMessages []*types.SignedMessage
//
//    Cids []cid.Cid
//}
