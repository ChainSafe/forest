// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use encoding::tuple::*;

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
