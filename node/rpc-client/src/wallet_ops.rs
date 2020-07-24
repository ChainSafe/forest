// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use jsonrpc_v2::{Error as JsonRpcError};
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use crypto::signature::json::SignatureJson;
use wallet::{json::KeyInfoJson};

const URL: &str = "http://127.0.0.1:1234/rpc/v0";

pub async fn new() -> Result<String, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_new(&mut client)
        .await?)
}

pub async fn list() -> Result<Vec<String>, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_list(&mut client)
        .await?)
}

pub async fn balance() -> Result<String, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_balance(&mut client)
        .await?)
}

pub async fn set_default() -> Result<(), JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_set_default(&mut client)
        .await?)
}

pub async fn default() -> Result<String, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_default(&mut client)
        .await?)
}

pub async fn sign() -> Result<SignatureJson, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_sign(&mut client)
        .await?)
}

pub async fn verify() -> Result<bool, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_verify(&mut client)
        .await?)
}

pub async fn import() -> Result<String, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_import(&mut client)
        .await?)
}

pub async fn export() -> Result<KeyInfoJson, JsonRpcError> {
    let mut client = RawClient::new(HttpTransportClient::new(URL));
        Ok(Filecoin::wallet_export(&mut client)
        .await?)
}