// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
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
