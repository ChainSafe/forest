// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use jsonrpc_v2::Error;
use rpc_api::mpool_api::*;

pub async fn mpool_pending(params: MpoolPendingParams) -> Result<MpoolPendingResult, Error> {
    call(MPOOL_PENDING, params).await
}
