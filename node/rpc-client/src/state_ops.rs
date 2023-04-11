// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_rpc_api::state_api::*;
use jsonrpc_v2::Error;

use crate::call;

pub async fn state_get_actor(
    params: StateGetActorParams,
    auth_token: &Option<String>,
) -> Result<StateGetActorResult, Error> {
    call(STATE_GET_ACTOR, params, auth_token).await
}
