// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use forest_rpc_api::mpool_api::*;
use jsonrpc_v2::Error;

pub async fn mpool_pending(params: MpoolPendingParams) -> Result<MpoolPendingResult, Error> {
    call(MPOOL_PENDING, params).await
}
