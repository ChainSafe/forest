// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{make_empty_map, make_map_with_root_and_bitwidth, FIRST_NON_SINGLETON_ADDR};
use address::{Address, Protocol};
use cid::Cid;
use encoding::tuple::*;
use encoding::Cbor;
use fil_types::{ActorID, HAMT_BIT_WIDTH};
use ipld_blockstore::BlockStore;
use ipld_hamt::Error as HamtError;
use std::error::Error as StdError;

/// State is reponsible for creating
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    pub address_map: Cid,
    pub next_id: ActorID,
    pub network_name: String,
}

impl State {
    pub fn new<BS: BlockStore>(
        store: &BS,
        network_name: String,
    ) -> Result<Self, Box<dyn StdError>> {
        let empty_map = make_empty_map::<_, ()>(store, HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| format!("failed to create empty map: {}", e))?;
        Ok(Self {
            address_map: empty_map,
            next_id: FIRST_NON_SINGLETON_ADDR,
            network_name,
        })
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

        let mut map = make_map_with_root_and_bitwidth(&self.address_map, store, HAMT_BIT_WIDTH)?;
        map.set(addr.to_bytes().into(), id)?;
        self.address_map = map.flush()?;

        Ok(Address::new_id(id))
    }

    /// ResolveAddress resolves an address to an ID-address, if possible.
    /// If the provided address is an ID address, it is returned as-is.
    /// This means that mapped ID-addresses (which should only appear as values, not keys) and
    /// singleton actor addresses (which are not in the map) pass through unchanged.
    ///
    /// Returns an ID-address and `true` if the address was already an ID-address or was resolved
    /// in the mapping.
    /// Returns an undefined address and `false` if the address was not an ID-address and not found
    /// in the mapping.
    /// Returns an error only if state was inconsistent.
    pub fn resolve_address<BS: BlockStore>(
        &self,
        store: &BS,
        addr: &Address,
    ) -> Result<Option<Address>, Box<dyn StdError>> {
        if addr.protocol() == Protocol::ID {
            return Ok(Some(*addr));
        }

        let map = make_map_with_root_and_bitwidth(&self.address_map, store, HAMT_BIT_WIDTH)?;

        Ok(map.get(&addr.to_bytes())?.copied().map(Address::new_id))
    }
}

impl Cbor for State {}
