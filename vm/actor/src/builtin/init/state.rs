// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{BytesKey, FIRST_NON_SINGLETON_ADDR, HAMT_BIT_WIDTH};
use address::{Address, Protocol};
use cid::Cid;
use encoding::Cbor;
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

    /// Allocates a new ID address and stores a mapping of the argument address to it.
    /// Returns the newly-allocated address.
    pub fn map_address_to_new_id<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
    ) -> Result<Address, HamtError> {
        let id = self.next_id;
        self.next_id += 1;

        let mut map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&self.address_map, store, HAMT_BIT_WIDTH)?;
        map.set(addr.to_bytes().into(), id)?;
        self.address_map = map.flush()?;

        Ok(Address::new_id(id).expect("Id Address should be created without Error"))
    }

    /// ResolveAddress resolves an address to an ID-address, if possible.
    /// If the provided address is an ID address, it is returned as-is.
    /// This means that ID-addresses (which should only appear as values, not keys)
    /// and singleton actor addresses pass through unchanged.
    ///
    /// Post-condition: all addresses succesfully returned by this method satisfy `addr.protocol() == Protocol::ID`.
    pub fn resolve_address<BS: BlockStore>(
        &self,
        store: &BS,
        addr: &Address,
    ) -> Result<Address, String> {
        if addr.protocol() == Protocol::ID {
            return Ok(addr.clone());
        }

        let map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&self.address_map, store, HAMT_BIT_WIDTH)?;

        let actor_id: ActorID = map
            .get(&addr.to_bytes())?
            .ok_or_else(|| "Address not found".to_owned())?;

        Ok(Address::new_id(actor_id).map_err(|e| e.to_string())?)
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

impl Cbor for State {}
