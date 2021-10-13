// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use jsonrpc_v2::Error;
use rpc_api::state_api::*;

pub async fn state_get_actor(params: StateGetActorParams) -> Result<StateGetActorResult, Error> {
    call(STATE_GET_ACTOR, params).await
}

pub async fn state_miner_power(
    params: StateMinerPowerParams,
) -> Result<StateMinerPowerResult, Error> {
    call(STATE_MINER_POWER, params).await
}

pub async fn state_list_actors(
    params: StateListActorsParams,
) -> Result<StateListActorsResult, Error> {
    call(STATE_LIST_ACTORS, params).await
}

pub async fn state_lookup(params: StateLookupIdParams) -> Result<StateLookupIdResult, Error> {
    call(STATE_LOOKUP_ID, params).await
}

pub async fn state_account_key(
    params: StateAccountKeyParams,
) -> Result<StateAccountKeyResult, Error> {
    call(STATE_ACCOUNT_KEY, params).await
}
