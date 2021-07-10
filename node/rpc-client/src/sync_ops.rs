// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::filecoin_rpc;

use jsonrpc_v2::Error as JsonRpcError;
use rpc_api::sync_api::*;

pub async fn sync_check_bad(
    params: SyncCheckBadParams,
) -> Result<SyncCheckBadResult, JsonRpcError> {
    filecoin_rpc::sync_check_bad(params).await
}

pub async fn sync_mark_bad(params: SyncMarkBadParams) -> Result<SyncMarkBadResult, JsonRpcError> {
    filecoin_rpc::sync_mark_bad(params).await
}

pub async fn sync_status(params: SyncStateParams) -> Result<SyncStateResult, JsonRpcError> {
    filecoin_rpc::sync_state(params).await
}
