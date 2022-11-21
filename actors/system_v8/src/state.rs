// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;

/// System actor state.
#[derive(Default, Deserialize_tuple, Serialize_tuple)]
pub struct State {
    // builtin actor registry: Vec<(String, Cid)>
    pub builtin_actors: Cid,
}
impl Cbor for State {}
