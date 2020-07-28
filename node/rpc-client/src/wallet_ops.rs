// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use jsonrpc_v2::{Error as JsonRpcError};
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;
use crypto::signature::json::SignatureJson;
use wallet::{json::KeyInfoJson};

/// Creates a new address in the wallet with the given signature type
pub async fn new(mut client: RawClient<HTC>) -> Result<String, JsonRpcError> {
        Ok(Filecoin::wallet_new(&mut client)
        .await?)
}

/// Lists all the addresses in the wallet
pub async fn list(mut client: RawClient<HTC>) -> Result<Vec<String>, JsonRpcError> {
        Ok(Filecoin::wallet_list(&mut client)
        .await?)
}

/// Returns the balance of the given address at the current head of the chain
pub async fn balance(mut client: RawClient<HTC>) -> Result<String, JsonRpcError> {
        Ok(Filecoin::wallet_balance(&mut client)
        .await?)
}

/// Marks the given address as as the default one
pub async fn set_default(mut client: RawClient<HTC>) -> Result<(), JsonRpcError> {
        Ok(Filecoin::wallet_set_default(&mut client)
        .await?)
}

/// Returns the address marked as default in the wallet
pub async fn default(mut client: RawClient<HTC>) -> Result<String, JsonRpcError> {
        Ok(Filecoin::wallet_default(&mut client)
        .await?)
}

/// Signs the given bytes using the given address
pub async fn sign(mut client: RawClient<HTC>) -> Result<SignatureJson, JsonRpcError> {
        Ok(Filecoin::wallet_sign(&mut client)
        .await?)
}

/// Takes an address, a signature, and some bytes, and indicates whether the signature is valid.
/// The address does not have to be in the wallet
pub async fn verify(mut client: RawClient<HTC>) -> Result<bool, JsonRpcError> {
        Ok(Filecoin::wallet_verify(&mut client)
        .await?)
}

/// Receives a KeyInfo, which includes a private key, and imports it into the wallet
pub async fn import(mut client: RawClient<HTC>) -> Result<String, JsonRpcError> {
        Ok(Filecoin::wallet_import(&mut client)
        .await?)
}

/// Returns the private key of an address in the wallet
pub async fn export(mut client: RawClient<HTC>) -> Result<KeyInfoJson, JsonRpcError> {
        Ok(Filecoin::wallet_export(&mut client)
        .await?)
}