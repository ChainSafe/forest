// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use cid::{json::CidJson, Cid};
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;
use rpc::RPCSyncState;
use serde_json::from_str;
use blocks::gossip_block::json::GossipBlockJson;

pub async fn mark_bad(client: &mut RawClient<HTC>, block_cid: String) -> Result<(), JsonRpcError> {
    let valid_cid = Cid::from_raw_cid(block_cid)?;
    Ok(Filecoin::mark_bad(client, CidJson(valid_cid)).await?)
}

pub async fn check_bad(
    client: &mut RawClient<HTC>,
    block_cid: String,
) -> Result<String, JsonRpcError> {
    let valid_cid = Cid::from_raw_cid(block_cid)?;
    Ok(Filecoin::check_bad(client, CidJson(valid_cid)).await?)
}

pub async fn status(client: &mut RawClient<HTC>) -> Result<RPCSyncState, JsonRpcError> {
    Ok(Filecoin::status(client).await?)
}

pub async fn submit_block(client: &mut RawClient<HTC>, gossip_block : String) -> Result<(), JsonRpcError> {
    let block_json: GossipBlockJson = from_str(&gossip_block)? ;
    Ok(Filecoin::submit_block(client, block_json ).await?)
}