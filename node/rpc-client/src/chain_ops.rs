// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::filecoin_rpc;
use cid::{json::CidJson, Cid};
use rpc_api::chain_api::*;

use jsonrpc_v2::Error as JsonRpcError;

/// Returns a block with specified CID fom chain via RPC
pub async fn block(cid: Cid) -> Result<ChainGetBlockResult, JsonRpcError> {
    filecoin_rpc::chain_get_block((CidJson(cid),)).await
}

/// Returns genesis tipset from chain via RPC
pub async fn genesis() -> Result<ChainGetGenesisResult, JsonRpcError> {
    filecoin_rpc::chain_get_genesis().await
}

/// Returns canonical head of the chain via RPC
pub async fn head() -> Result<ChainHeadResult, JsonRpcError> {
    filecoin_rpc::chain_head().await
}

/// Returns messages with specified CID from chain via RPC
pub async fn messages(cid: Cid) -> Result<ChainGetMessageResult, JsonRpcError> {
    filecoin_rpc::chain_get_message((CidJson(cid),)).await
}

/// Returns IPLD node with specified CID from chain via RPC
pub async fn read_obj(cid: Cid) -> Result<ChainReadObjResult, JsonRpcError> {
    filecoin_rpc::chain_read_obj((CidJson(cid),)).await
}
