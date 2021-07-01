// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call_params;
use jsonrpc_v2::Error;
use rpc_api::auth_api::*;

pub async fn auth_new(perm: AuthNewParams) -> Result<AuthNewResult, Error> {
    call_params(AUTH_NEW, perm).await
}
