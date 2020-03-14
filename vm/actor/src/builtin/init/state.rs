// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FIRST_NON_SINGLETON_ADDR;
use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error as HamtError, Hamt};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::ActorID;

/// State is reponsible for creating
pub struct State {
    address_map: Cid,
    next_id: ActorID,
    network_name: String,
}

impl State {
    pub fn new(address_map: Cid, network_name: String) -> Self {
        Self {
            address_map,
            next_id: FIRST_NON_SINGLETON_ADDR,
            network_name,
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

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.address_map, &self.next_id, &self.network_name).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (address_map, next_id, network_name) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            address_map,
            next_id,
            network_name,
        })
    }
}
