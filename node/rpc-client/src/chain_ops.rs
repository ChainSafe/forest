// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use jsonrpc_v2::Error;
use rpc_api::chain_api::*;

pub async fn chain_get_block(cid: ChainGetBlockParams) -> Result<ChainGetBlockResult, Error> {
    call(CHAIN_GET_BLOCK, cid).await
}

pub async fn chain_get_genesis() -> Result<ChainGetGenesisResult, Error> {
    call(CHAIN_GET_GENESIS, ()).await
}

pub async fn chain_head() -> Result<ChainHeadResult, Error> {
    call(CHAIN_HEAD, ()).await
}

pub async fn chain_get_message(cid: ChainGetMessageParams) -> Result<ChainGetMessageResult, Error> {
    call(CHAIN_GET_MESSAGE, cid).await
}

pub async fn chain_read_obj(cid: ChainReadObjParams) -> Result<ChainReadObjResult, Error> {
    call(CHAIN_READ_OBJ, cid).await
}

pub async fn chain_get_tipset(keys: ChainGetTipSetParams) -> Result<ChainGetTipSetResult, Error> {
    call(CHAIN_GET_TIPSET, keys).await
}
