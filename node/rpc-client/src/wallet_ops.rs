// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::filecoin_rpc;
use jsonrpc_v2::Error as JsonRpcError;
use rpc_api::wallet_api::*;

pub async fn wallet_new(params: WalletNewParams) -> Result<WalletNewResult, JsonRpcError> {
    filecoin_rpc::wallet_new(params).await
}

pub async fn wallet_default_address() -> Result<WalletDefaultAddressResult, JsonRpcError> {
    filecoin_rpc::wallet_default_address().await
}

pub async fn wallet_balance(
    params: WalletBalanceParams,
) -> Result<WalletBalanceResult, JsonRpcError> {
    filecoin_rpc::wallet_balance(params).await
}

pub async fn wallet_export(params: WalletExportParams) -> Result<WalletExportResult, JsonRpcError> {
    filecoin_rpc::wallet_export(params).await
}

pub async fn wallet_import(params: WalletImportParams) -> Result<WalletImportResult, JsonRpcError> {
    filecoin_rpc::wallet_import(params).await
}

pub async fn wallet_list() -> Result<WalletListResult, JsonRpcError> {
    filecoin_rpc::wallet_list().await
}

pub async fn wallet_has(params: WalletHasParams) -> Result<WalletHasResult, JsonRpcError> {
    filecoin_rpc::wallet_has(params).await
}

pub async fn wallet_set_default(
    params: WalletSetDefaultParams,
) -> Result<WalletSetDefaultResult, JsonRpcError> {
    filecoin_rpc::wallet_set_default(params).await
}

pub async fn wallet_sign(params: WalletSignParams) -> Result<WalletSignResult, JsonRpcError> {
    filecoin_rpc::wallet_sign(params).await
}

pub async fn wallet_verify(params: WalletVerifyParams) -> Result<WalletVerifyResult, JsonRpcError> {
    filecoin_rpc::wallet_verify(params).await
}
