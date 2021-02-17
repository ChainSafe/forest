// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::error::Error as StdError;

use address::Address;
use cid::Cid;
use encoding::{tuple::*, Cbor};
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;

use crate::make_empty_map;

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    pub root_key: Address,
    pub verifiers: Cid,
    pub verified_clients: Cid,
}

impl State {
    pub fn new<BS: BlockStore>(store: &BS, root_key: Address) -> Result<State, Box<dyn StdError>> {
        let empty_map = make_empty_map::<_, ()>(store, HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| format!("Failed to create empty map: {}", e))?;

        Ok(State {
            root_key,
            verifiers: empty_map,
            verified_clients: empty_map,
        })
    }
}

impl Cbor for State {}
