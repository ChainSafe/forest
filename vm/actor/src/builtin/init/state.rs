// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FIRST_NON_SINGLETON_ADDR;
use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error as HamtError, Hamt};
use serde::{Deserialize, Serialize};
use vm::ActorID;

/// InitActorState is reponsible for creating
// TODO implement actual serialize and deserialize to match
#[derive(Serialize, Deserialize)]
pub struct InitActorState {
    address_map: Cid,
    next_id: ActorID,
}

impl InitActorState {
    pub fn new(address_map: Cid) -> Self {
        Self {
            address_map,
            next_id: FIRST_NON_SINGLETON_ADDR,
        }
    }
    /// Assigns next available ID and incremenets the next_id value from state
    pub fn map_address_to_new_id<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
    ) -> Result<Address, HamtError> {
        let id = self.next_id;
        self.next_id += 1;

        let mut map: Hamt<String, _> = Hamt::load_with_bit_width(&self.address_map, store, 5)?;
        map.set(String::from_utf8_lossy(&addr.to_bytes()).to_string(), id)?;
        self.address_map = map.flush()?;

        Ok(Address::new_id(id.0).expect("Id Address should be created without Error"))
    }

    /// Resolve address
    pub fn resolve_address<BS: BlockStore>(
        &self,
        _store: &BS,
        _addr: &Address,
    ) -> Result<Address, String> {
        // TODO implement address resolution
        todo!()
    }
}
