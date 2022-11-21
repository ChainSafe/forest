// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fil_actors_runtime_v9::{
    actor_error, make_empty_map, make_map_with_root_and_bitwidth, ActorError, AsActorError,
    FIRST_NON_SINGLETON_ADDR,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::{Address, Protocol};
use fvm_shared::error::ExitCode;
use fvm_shared::{ActorID, HAMT_BIT_WIDTH};

/// State is reponsible for creating
#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
pub struct State {
    pub address_map: Cid,
    pub next_id: ActorID,
    pub network_name: String,
}

impl State {
    pub fn new<BS: Blockstore>(store: &BS, network_name: String) -> Result<Self, ActorError> {
        let empty_map = make_empty_map::<_, ()>(store, HAMT_BIT_WIDTH)
            .flush()
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to create empty map")?;
        Ok(Self {
            address_map: empty_map,
            next_id: FIRST_NON_SINGLETON_ADDR,
            network_name,
        })
    }

    /// Allocates a new ID address and stores a mapping of the argument address to it.
    /// Fails if the argument address is already present in the map to facilitate a tombstone
    /// for when the predictable robust address generation is implemented.
    /// Returns the newly-allocated address.
    pub fn map_address_to_new_id<BS: Blockstore>(
        &mut self,
        store: &BS,
        addr: &Address,
    ) -> Result<ActorID, ActorError> {
        let id = self.next_id;
        self.next_id += 1;

        let mut map = make_map_with_root_and_bitwidth(&self.address_map, store, HAMT_BIT_WIDTH)
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load address map")?;
        let is_new = map
            .set_if_absent(addr.to_bytes().into(), id)
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to set map key")?;
        if !is_new {
            // this is impossible today as the robust address is a hash of unique inputs
            // but in close future predictable address generation will make this possible
            return Err(actor_error!(
                forbidden,
                "robust address {} is already allocated in the address map",
                addr
            ));
        }
        self.address_map = map
            .flush()
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to store address map")?;

        Ok(id)
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
    pub fn resolve_address<BS: Blockstore>(
        &self,
        store: &BS,
        addr: &Address,
    ) -> Result<Option<Address>, ActorError> {
        if addr.protocol() == Protocol::ID {
            return Ok(Some(*addr));
        }

        let map = make_map_with_root_and_bitwidth(&self.address_map, store, HAMT_BIT_WIDTH)
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load address map")?;

        let found = map
            .get(&addr.to_bytes())
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to get address entry")?;
        Ok(found.copied().map(Address::new_id))
    }
}

impl Cbor for State {}
