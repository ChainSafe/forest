// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use encoding::{tuple::*, Cbor};

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    pub root_key: Address,
    pub verifiers: Cid,
    pub verified_clients: Cid,
}

impl State {
    pub fn new(empty_map: Cid, root_key: Address) -> State {
        State {
            root_key,
            verifiers: empty_map.clone(),
            verified_clients: empty_map,
        }
    }
}

impl Cbor for State {}
