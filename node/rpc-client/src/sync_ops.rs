// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use jsonrpc_v2::Error as JsonRpcError;
use rpc_api::sync_api::*;

pub async fn sync_check_bad(
    params: SyncCheckBadParams,
) -> Result<SyncCheckBadResult, JsonRpcError> {
    call(SYNC_CHECK_BAD, params).await
}

pub async fn sync_mark_bad(params: SyncMarkBadParams) -> Result<SyncMarkBadResult, JsonRpcError> {
    call(SYNC_MARK_BAD, params).await
}

pub async fn sync_status(params: SyncStateParams) -> Result<SyncStateResult, JsonRpcError> {
    call(SYNC_STATE, params).await
}
