// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use forest_rpc_api::state_api::*;
use jsonrpc_v2::Error;

pub async fn state_get_actor(
    params: StateGetActorParams,
    auth_token: &Option<String>,
) -> Result<StateGetActorResult, Error> {
    call(STATE_GET_ACTOR, params, auth_token).await
}

pub async fn state_miner_power(
    params: StateMinerPowerParams,
    auth_token: &Option<String>,
) -> Result<StateMinerPowerResult, Error> {
    call(STATE_MINER_POWER, params, auth_token).await
}

pub async fn state_list_actors(
    params: StateListActorsParams,
    auth_token: &Option<String>,
) -> Result<StateListActorsResult, Error> {
    call(STATE_LIST_ACTORS, params, auth_token).await
}

pub async fn state_lookup(
    params: StateLookupIdParams,
    auth_token: &Option<String>,
) -> Result<StateLookupIdResult, Error> {
    call(STATE_LOOKUP_ID, params, auth_token).await
}

pub async fn state_account_key(
    params: StateAccountKeyParams,
    auth_token: &Option<String>,
) -> Result<StateAccountKeyResult, Error> {
    call(STATE_ACCOUNT_KEY, params, auth_token).await
}
