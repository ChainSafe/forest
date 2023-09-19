// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// ported from commit hash b622af

// The FVM crates only support state tree versions 3,4 and 5. This module
// contains read-only support for state tree version 0. This version is required
// to parse genesis states. Ideally, we would have a library that supports _all_
// state tree versions.

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use super::state_tree::StateTreeVersion;
use crate::shim::address::Address;
use fil_actors_shared::fvm_ipld_hamt::Hamtv0 as Hamt;
pub use fvm2::state_tree::ActorState as ActorStateV2;
pub use fvm_shared3::state::StateRoot;

const HAMTV0_BIT_WIDTH: u32 = 5;

// This is a read-only version of the earliest state trees.
/// State tree implementation using HAMT. This structure is not thread safe and should only be used
/// in sync contexts.
pub struct StateTreeV0<S> {
    hamt: Hamt<S, ActorStateV2>,
}

impl<S> StateTreeV0<S>
where
    S: Blockstore,
{
    /// Constructor for a HAMT state tree given an IPLD store
    pub fn new_from_root(store: S, c: &Cid) -> anyhow::Result<Self> {
        // Try to load state root, if versioned
        let (version, actors) = if let Ok(Some(StateRoot {
            version, actors, ..
        })) = store.get_cbor(c)
        {
            (StateTreeVersion::from(version), actors)
        } else {
            // Fallback to v0 state tree if retrieval fails
            (StateTreeVersion::V0, *c)
        };

        match version {
            StateTreeVersion::V0 => {
                let hamt = Hamt::load_with_bit_width(&actors, store, HAMTV0_BIT_WIDTH)?;
                Ok(Self { hamt })
            }
            _ => anyhow::bail!("unsupported state tree version: {:?}", version),
        }
    }

    /// Retrieve store reference to modify db.
    pub fn store(&self) -> &S {
        self.hamt.store()
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> anyhow::Result<Option<ActorStateV2>> {
        let addr = match self.lookup_id(addr)? {
            Some(addr) => addr,
            None => return Ok(None),
        };

        // if state doesn't exist, find using hamt
        let act = self.hamt.get(&addr.to_bytes())?.cloned();

        Ok(act)
    }

    /// Get an ID address from any Address
    pub fn lookup_id(&self, addr: &Address) -> anyhow::Result<Option<Address>> {
        if addr.protocol() == fvm_shared3::address::Protocol::ID {
            return Ok(Some(*addr));
        }
        anyhow::bail!("StateTreeV0::lookup_id is only defined for ID addresses")
    }
}
