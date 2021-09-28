// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::state::State;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::tuple::*;
use num_bigint::bigint_ser;
use vm::{ExitCode, Serialized, TokenAmount};

/// CreateActorArgs are the arguments to CreateActor.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CreateActorArgs {
    pub undef_cid: bool,
    pub cid: Cid,
    pub undef_address: bool,
    pub address: Address,
}

/// Holds the response of a call to runtime.ResolveAddress
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ResolveAddressResponse {
    pub address: Address,
    pub success: bool,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SendArgs {
    pub to: Address,
    #[serde(with = "bigint_ser")]
    pub value: TokenAmount,
    #[serde(rename = "MethodNum")]
    pub method: u64,
    pub params: Serialized,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
// SendReturn is the return values for the Send method.
pub struct SendReturn {
    pub return_value: Serialized,
    pub code: ExitCode,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct MutateStateArgs {
    pub value: String,
    pub branch: i64,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct AbortWithArgs {
    pub code: ExitCode,
    pub message: String,
    pub uncontrolled: bool,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct InspectRuntimeReturn {
    pub caller: Address,
    pub receiver: Address,
    #[serde(with = "bigint_ser")]
    pub value_received: TokenAmount,
    pub curr_epoch: ChainEpoch,
    #[serde(with = "bigint_ser")]
    pub current_balance: TokenAmount,
    pub state: State,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CallerValidationArgs {
    pub branch: i64,
    pub addrs: Vec<Address>,
    pub types: Vec<Cid>,
}
