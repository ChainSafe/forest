// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(unused_variables)]
#![allow(dead_code)]

use super::client::Filecoin;
use blocks::{
    tipset_json::TipsetJson, header::json::BlockHeaderJson
};
use message::{
    unsigned_message::{json::UnsignedMessageJson},
};
use jsonrpc_v2::{Error as JsonRpcError};
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;

const URL: &str = "http://127.0.0.1:1234/rpc/v0";

pub async fn get_genesis() -> Result<TipsetJson, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::chain_get_genesis(&mut client)
        .await?)
}

pub async fn get_messages(cid: String) -> Result<UnsignedMessageJson, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::chain_get_messages(&mut client, cid)
        .await?)
}

pub async fn get_head() -> Result<TipsetJson, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::chain_get_head(&mut client)
        .await?)
}

pub async fn get_block(cid: String) -> Result<BlockHeaderJson, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::chain_get_block(&mut client, cid)
        .await?)
}

pub async fn read_obj(cid: String) -> Result<Vec<u8>, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::chain_read_obj(&mut client, cid)
        .await?)
}