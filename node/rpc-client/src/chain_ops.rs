// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::filecoin_rpc;
use blocks::{header::json::BlockHeaderJson, tipset_json::TipsetJson};
use cid::{json::CidJson, Cid};
use jsonrpc_v2::Error as JsonRpcError;
use message::unsigned_message::json::UnsignedMessageJson;

/// Returns a block with specified CID fom chain via RPC
pub async fn block(cid: Cid) -> Result<BlockHeaderJson, JsonRpcError> {
    filecoin_rpc::chain_get_block(CidJson(cid)).await
}

/// Returns genesis tipset from chain via RPC
pub async fn genesis() -> Result<TipsetJson, JsonRpcError> {
    filecoin_rpc::chain_get_genesis().await
}

/// Returns canonical head of the chain via RPC
pub async fn head() -> Result<TipsetJson, JsonRpcError> {
    filecoin_rpc::chain_get_head().await
}

/// Returns messages with specified CID from chain via RPC
pub async fn messages(cid: Cid) -> Result<UnsignedMessageJson, JsonRpcError> {
    filecoin_rpc::chain_get_messages(CidJson(cid)).await
}

/// Returns IPLD node with specified CID from chain via RPC
pub async fn read_obj(cid: Cid) -> Result<Vec<u8>, JsonRpcError> {
    filecoin_rpc::chain_read_obj(CidJson(cid)).await
}
