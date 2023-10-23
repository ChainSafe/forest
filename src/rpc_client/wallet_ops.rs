// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    key_management::KeyInfo,
    rpc_api::wallet_api::*,
    shim::{address::Address, crypto::SignatureType},
};
use jsonrpc_v2::Error;

use crate::rpc_client::call;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn wallet_default_address(&self) -> Result<Option<String>, JsonRpcError> {
        self.call_req_e(Self::wallet_default_address_req()).await
    }

    pub fn wallet_default_address_req() -> RpcRequest<Option<String>> {
        RpcRequest::new(WALLET_DEFAULT_ADDRESS, ())
    }
}

pub async fn wallet_new(
    signature_type: WalletNewParams,
    auth_token: &Option<String>,
) -> Result<WalletNewResult, Error> {
    call(WALLET_NEW, signature_type, auth_token).await
}

pub fn wallet_new_req(signature_type: SignatureType) -> RpcRequest<String> {
    RpcRequest::new(WALLET_NEW, (signature_type,))
}

pub async fn wallet_default_address(
    (): WalletDefaultAddressParams,
    auth_token: &Option<String>,
) -> Result<WalletDefaultAddressResult, Error> {
    call(WALLET_DEFAULT_ADDRESS, (), auth_token).await
}

pub fn wallet_default_address_req() -> RpcRequest<Option<String>> {
    RpcRequest::new(WALLET_DEFAULT_ADDRESS, ())
}

pub async fn wallet_balance(
    address: WalletBalanceParams,
    auth_token: &Option<String>,
) -> Result<WalletBalanceResult, Error> {
    call(WALLET_BALANCE, address, auth_token).await
}

pub fn wallet_balance_req(address: String) -> RpcRequest<String> {
    RpcRequest::new(WALLET_BALANCE, address)
}

pub async fn wallet_export(
    address: WalletExportParams,
    auth_token: &Option<String>,
) -> Result<WalletExportResult, Error> {
    call(WALLET_EXPORT, address, auth_token).await
}

pub fn wallet_export_req(address: String) -> RpcRequest<KeyInfo> {
    RpcRequest::new(WALLET_EXPORT, address)
}

pub async fn wallet_import(
    key: WalletImportParams,
    auth_token: &Option<String>,
) -> Result<WalletImportResult, Error> {
    call(WALLET_IMPORT, key, auth_token).await
}

pub fn wallet_import_req(key: Vec<KeyInfo>) -> RpcRequest<String> {
    RpcRequest::new(WALLET_IMPORT, key)
}

pub async fn wallet_list(
    (): WalletListParams,
    auth_token: &Option<String>,
) -> Result<WalletListResult, Error> {
    call(WALLET_LIST, (), auth_token).await
}

pub fn wallet_list_req() -> RpcRequest<Vec<Address>> {
    RpcRequest::new(WALLET_LIST, ())
}

pub async fn wallet_has(
    key: WalletHasParams,
    auth_token: &Option<String>,
) -> Result<WalletHasResult, Error> {
    call(WALLET_HAS, key, auth_token).await
}

pub fn wallet_has_req(key: String) -> RpcRequest<bool> {
    RpcRequest::new(WALLET_HAS, key)
}

pub async fn wallet_set_default(
    address: WalletSetDefaultParams,
    auth_token: &Option<String>,
) -> Result<WalletSetDefaultResult, Error> {
    call(WALLET_SET_DEFAULT, address, auth_token).await
}

pub fn wallet_set_default_req(address: Address) -> RpcRequest<WalletSetDefaultResult> {
    RpcRequest::new(WALLET_SET_DEFAULT, (address,))
}

pub async fn wallet_sign(
    message: WalletSignParams,
    auth_token: &Option<String>,
) -> Result<WalletSignResult, Error> {
    call(WALLET_SIGN, message, auth_token).await
}

pub async fn wallet_verify(
    message: WalletVerifyParams,
    auth_token: &Option<String>,
) -> Result<WalletVerifyResult, Error> {
    call(WALLET_VERIFY, message, auth_token).await
}

pub async fn wallet_delete(
    message: WalletDeleteParams,
    auth_token: &Option<String>,
) -> Result<WalletDeleteResult, Error> {
    call(WALLET_DELETE, message, auth_token).await
}
