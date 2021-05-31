// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::filecoin_rpc;
use cid::{json::CidJson, Cid};
use jsonrpc_v2::Error as JsonRpcError;

/// Returns a block with specified CID fom chain via RPC
pub async fn block(
    cid: Cid,
) -> Result<rpc_api::chain_get_block::ChainGetBlockResult, JsonRpcError> {
    filecoin_rpc::chain_get_block((CidJson(cid),)).await
}

/// Returns genesis tipset from chain via RPC
pub async fn genesis() -> Result<rpc_api::chain_get_genesis::ChainGetGenesisResult, JsonRpcError> {
    filecoin_rpc::chain_get_genesis().await
}

/// Returns canonical head of the chain via RPC
pub async fn head() -> Result<rpc_api::chain_head::ChainHeadResult, JsonRpcError> {
    filecoin_rpc::chain_head().await
}

/// Returns messages with specified CID from chain via RPC
pub async fn messages(
    cid: Cid,
) -> Result<rpc_api::chain_get_message::ChainGetMessageResult, JsonRpcError> {
    filecoin_rpc::chain_get_message((CidJson(cid),)).await
}

/// Returns IPLD node with specified CID from chain via RPC
pub async fn read_obj(
    cid: Cid,
) -> Result<rpc_api::chain_read_obj::ChainReadObjResult, JsonRpcError> {
    filecoin_rpc::chain_read_obj((CidJson(cid),)).await
}
