// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_rpc_api::db_api::*;
use jsonrpc_v2::Error;

use crate::call;

pub async fn db_gc(params: DBGCParams, auth_token: &Option<String>) -> Result<DBGCResult, Error> {
    call(DB_GC, params, auth_token).await
}
