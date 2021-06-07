// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::filecoin_rpc;
use address::{json::AddressJson, Address};
use crypto::{
    signature::json::signature_type::SignatureTypeJson, signature::json::SignatureJson, Signature,
};
use jsonrpc_v2::Error as JsonRpcError;
use wallet::{json::KeyInfoJson, KeyInfo};

pub async fn wallet_new(signature_type: SignatureTypeJson) -> Result<String, JsonRpcError> {
    filecoin_rpc::wallet_new((signature_type,)).await
}

pub async fn wallet_default_address() -> Result<String, JsonRpcError> {
    filecoin_rpc::wallet_default_address().await
}

pub async fn wallet_balance(address: String) -> Result<String, JsonRpcError> {
    filecoin_rpc::wallet_balance((address,)).await
}

pub async fn wallet_export(address: String) -> Result<KeyInfoJson, JsonRpcError> {
    filecoin_rpc::wallet_export((address,)).await
}

pub async fn wallet_import(key: KeyInfo) -> Result<KeyInfoJson, JsonRpcError> {
    filecoin_rpc::wallet_import(vec![KeyInfoJson(key)]).await
}

pub async fn wallet_list() -> Result<Vec<AddressJson>, JsonRpcError> {
    filecoin_rpc::wallet_list().await
}

pub async fn wallet_has(key: String) -> Result<bool, JsonRpcError> {
    filecoin_rpc::wallet_has((key,)).await
}

pub async fn wallet_set_default(key: Address) -> Result<(), JsonRpcError> {
    filecoin_rpc::wallet_set_default((AddressJson(key),)).await
}

pub async fn wallet_sign(
    address: Address,
    message: Vec<u8>,
) -> Result<SignatureJson, JsonRpcError> {
    filecoin_rpc::wallet_sign((AddressJson(address), message)).await
}

pub async fn wallet_verify(
    message: String,
    address: String,
    signature: Signature,
) -> Result<bool, JsonRpcError> {
    filecoin_rpc::wallet_verify((message, address, SignatureJson(signature))).await
}
