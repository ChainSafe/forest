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
pub async fn block(mut client: RawClient<HTC>, cid: Cid) -> Result<BlockHeaderJson, JsonRpcError> {
    Ok(Filecoin::chain_get_block(&mut client, CidJson(cid)).await?)
}

/// Returns genesis tipset from chain via RPC
pub async fn genesis(mut client: RawClient<HTC>) -> Result<TipsetJson, JsonRpcError> {
    Ok(Filecoin::chain_get_genesis(&mut client).await?)
}

/// Returns canonical head of the chain via RPC
pub async fn head(mut client: RawClient<HTC>) -> Result<TipsetJson, JsonRpcError> {
    Ok(Filecoin::chain_get_head(&mut client).await?)
}

/// Returns messages with specified CID from chain via RPC
pub async fn messages(
    mut client: RawClient<HTC>,
    cid: Cid,
) -> Result<UnsignedMessageJson, JsonRpcError> {
    Ok(Filecoin::chain_get_messages(&mut client, CidJson(cid)).await?)
}

/// Returns IPLD node with specified CID from chain via RPC
pub async fn read_obj(mut client: RawClient<HTC>, cid: Cid) -> Result<Vec<u8>, JsonRpcError> {
    Ok(Filecoin::chain_read_obj(&mut client, CidJson(cid)).await?)
}