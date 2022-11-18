// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::ExitCode;
use fvm_shared::ActorID;

use super::state::State;

/// CreateActorArgs are the arguments to CreateActor.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CreateActorArgs {
    pub undef_cid: bool,
    pub cid: Cid,
    pub undef_address: bool,
    pub actor_id: ActorID,
}

/// Holds the response of a call to runtime.ResolveAddress
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ResolveAddressResponse {
    pub id: ActorID,
    pub success: bool,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SendArgs {
    pub to: Address,
    pub value: TokenAmount,
    #[serde(rename = "MethodNum")]
    pub method: u64,
    pub params: RawBytes,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
// SendReturn is the return values for the Send method.
pub struct SendReturn {
    pub return_value: RawBytes,
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
    pub value_received: TokenAmount,
    pub curr_epoch: ChainEpoch,
    pub current_balance: TokenAmount,
    pub state: State,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CallerValidationArgs {
    pub branch: i64,
    pub addrs: Vec<Address>,
    pub types: Vec<Cid>,
}
