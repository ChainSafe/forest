// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::mpool_api::*;
use jsonrpc_v2::Error;

use crate::rpc_client::call;

pub async fn mpool_push_message(
    params: MpoolPushMessageParams,
    auth_token: &Option<String>,
) -> Result<MpoolPushMessageResult, Error> {
    call(MPOOL_PUSH_MESSAGE, params, auth_token).await
}
