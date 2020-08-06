// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use crypto::signature::json::SignatureJson;
use crypto::signature::SignatureType;
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;
use wallet::json::KeyInfoJson;

/// Creates a new address in the wallet with the given signature type
pub async fn new(
    client: &mut RawClient<HTC>,
    sig_type: SignatureType,
) -> Result<String, JsonRpcError> {
    Ok(Filecoin::wallet_new(&mut client, sig_type).await?)
}

/// Lists all the addresses in the wallet
pub async fn list(client: &mut RawClient<HTC>) -> Result<Vec<String>, JsonRpcError> {
    Ok(Filecoin::wallet_list(&mut client).await?)
}

/// Returns the balance of the given address at the current head of the chain
pub async fn balance(client: &mut RawClient<HTC>, address: String) -> Result<String, JsonRpcError> {
    Ok(Filecoin::wallet_balance(&mut client, address).await?)
}

/// Marks the given address as as the default one
pub async fn set_default(client: &mut RawClient<HTC>, address: String) -> Result<(), JsonRpcError> {
    Ok(Filecoin::wallet_set_default(&mut client, address).await?)
}

/// Returns the address marked as default in the wallet
pub async fn default(client: &mut RawClient<HTC>) -> Result<String, JsonRpcError> {
    Ok(Filecoin::wallet_default(&mut client).await?)
}

/// Signs the given bytes using the given address
pub async fn sign(
    client: &mut RawClient<HTC>,
    params: (String, String),
) -> Result<SignatureJson, JsonRpcError> {
    Ok(Filecoin::wallet_sign(&mut client, params).await?)
}

/// Takes an address, a signature, and some bytes, and indicates whether the signature is valid.
/// The address does not have to be in the wallet
pub async fn verify(
    client: &mut RawClient<HTC>,
    params: (String, String, SignatureJson),
) -> Result<bool, JsonRpcError> {
    Ok(Filecoin::wallet_verify(&mut client, params).await?)
}

/// Receives a KeyInfo, which includes a private key, and imports it into the wallet
pub async fn import(
    client: &mut RawClient<HTC>,
    key_info: KeyInfoJson,
) -> Result<String, JsonRpcError> {
    Ok(Filecoin::wallet_import(&mut client, key_info).await?)
}

/// Returns the private key of an address in the wallet
pub async fn export(
    client: &mut RawClient<HTC>,
    address: String,
) -> Result<KeyInfoJson, JsonRpcError> {
    Ok(Filecoin::wallet_export(&mut client, address).await?)
}
