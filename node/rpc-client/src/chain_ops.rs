// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use blocks::{header::json::BlockHeaderJson, tipset_json::TipsetJson};
use cid::{json::CidJson, Cid};
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;
use message::unsigned_message::json::UnsignedMessageJson;

/// Returns a block with specified CID fom chain via RPC
pub async fn block(client: &mut RawClient<HTC>, cid: Cid) -> Result<BlockHeaderJson, JsonRpcError> {
    Ok(Filecoin::chain_get_block(client, CidJson(cid)).await?)
}

/// Returns genesis tipset from chain via RPC
pub async fn genesis(client: &mut RawClient<HTC>) -> Result<TipsetJson, JsonRpcError> {
    Ok(Filecoin::chain_get_genesis(client).await?)
}

/// Returns canonical head of the chain via RPC
pub async fn head(client: &mut RawClient<HTC>) -> Result<TipsetJson, JsonRpcError> {
    Ok(Filecoin::chain_get_head(client).await?)
}

/// Returns messages with specified CID from chain via RPC
pub async fn messages(
    client: &mut RawClient<HTC>,
    cid: Cid,
) -> Result<UnsignedMessageJson, JsonRpcError> {
    Ok(Filecoin::chain_get_messages(client, CidJson(cid)).await?)
}

/// Returns IPLD node with specified CID from chain via RPC
pub async fn read_obj(client: &mut RawClient<HTC>, cid: Cid) -> Result<Vec<u8>, JsonRpcError> {
    Ok(Filecoin::chain_read_obj(client, CidJson(cid)).await?)
}
