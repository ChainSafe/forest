// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use jsonrpc_v2::Error;
use rpc_api::wallet_api::*;

pub async fn wallet_new(signature_type: WalletNewParams) -> Result<WalletNewResult, Error> {
    call(WALLET_NEW, signature_type).await
}

pub async fn wallet_default_address() -> Result<WalletDefaultAddressResult, Error> {
    call(WALLET_DEFAULT_ADDRESS, ()).await
}

pub async fn wallet_balance(address: WalletBalanceParams) -> Result<WalletBalanceResult, Error> {
    call(WALLET_BALANCE, address).await
}

pub async fn wallet_export(address: WalletExportParams) -> Result<WalletExportResult, Error> {
    call(WALLET_EXPORT, address).await
}

pub async fn wallet_import(key: WalletImportParams) -> Result<WalletImportResult, Error> {
    call(WALLET_IMPORT, key).await
}

pub async fn wallet_list() -> Result<WalletListResult, Error> {
    call(WALLET_LIST, ()).await
}

pub async fn wallet_has(key: WalletHasParams) -> Result<WalletHasResult, Error> {
    call(WALLET_HAS, key).await
}

pub async fn wallet_set_default(
    address: WalletSetDefaultParams,
) -> Result<WalletSetDefaultResult, Error> {
    call(WALLET_SET_DEFAULT, address).await
}

pub async fn wallet_sign(message: WalletSignParams) -> Result<WalletSignResult, Error> {
    call(WALLET_SIGN, message).await
}

pub async fn wallet_verify(message: WalletVerifyParams) -> Result<WalletVerifyResult, Error> {
    call(WALLET_VERIFY, message).await
}
