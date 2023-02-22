// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_rpc_api::wallet_api::*;
use jsonrpc_v2::Error;

use crate::call;

pub async fn wallet_new(
    signature_type: WalletNewParams,
    auth_token: &Option<String>,
) -> Result<WalletNewResult, Error> {
    call(WALLET_NEW, signature_type, auth_token).await
}

pub async fn wallet_default_address(
    params: WalletDefaultAddressParams,
    auth_token: &Option<String>,
) -> Result<WalletDefaultAddressResult, Error> {
    call(WALLET_DEFAULT_ADDRESS, params, auth_token).await
}

pub async fn wallet_balance(
    address: WalletBalanceParams,
    auth_token: &Option<String>,
) -> Result<WalletBalanceResult, Error> {
    call(WALLET_BALANCE, address, auth_token).await
}

pub async fn wallet_export(
    address: WalletExportParams,
    auth_token: &Option<String>,
) -> Result<WalletExportResult, Error> {
    call(WALLET_EXPORT, address, auth_token).await
}

pub async fn wallet_import(
    key: WalletImportParams,
    auth_token: &Option<String>,
) -> Result<WalletImportResult, Error> {
    call(WALLET_IMPORT, key, auth_token).await
}

pub async fn wallet_list(
    params: WalletListParams,
    auth_token: &Option<String>,
) -> Result<WalletListResult, Error> {
    call(WALLET_LIST, params, auth_token).await
}

pub async fn wallet_has(
    key: WalletHasParams,
    auth_token: &Option<String>,
) -> Result<WalletHasResult, Error> {
    call(WALLET_HAS, key, auth_token).await
}

pub async fn wallet_set_default(
    address: WalletSetDefaultParams,
    auth_token: &Option<String>,
) -> Result<WalletSetDefaultResult, Error> {
    call(WALLET_SET_DEFAULT, address, auth_token).await
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
