// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use forest_rpc_api::mpool_api::*;
use jsonrpc_v2::Error;

pub async fn mpool_pending(
    params: MpoolPendingParams,
    auth_token: &Option<String>,
) -> Result<MpoolPendingResult, Error> {
    call(MPOOL_PENDING, params, auth_token).await
}

pub async fn mpool_push_message(
    params: MpoolPushMessageParams,
    auth_token: &Option<String>,
) -> Result<MpoolPushMessageResult, Error> {
    call(MPOOL_PUSH_MESSAGE, params, auth_token).await
}
