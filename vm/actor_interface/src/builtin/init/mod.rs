// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_ipld_blockstore::{BlockStore, BlockStoreExt};
use fvm::state_tree::ActorState;
use fvm_shared::address::Address;
use serde::Serialize;

use anyhow::Context;

/// Init actor address.
pub const ADDRESS: Address = Address::new_id(1);

/// Init actor method.
pub type Method = fil_actor_init_v8::Method;

pub fn is_v8_init_cid(cid: &Cid) -> bool {
    let known_cids = vec![
        // calibnet
        Cid::try_from("bafk2bzaceadyfilb22bcvzvnpzbg2lyg6npmperyq6es2brvzjdh5rmywc4ry").unwrap(),
        // mainnet
        Cid::try_from("bafk2bzaceaipvjhoxmtofsnv3aj6gj5ida4afdrxa4ewku2hfipdlxpaektlw").unwrap(),
        // devnet
        Cid::try_from("bafk2bzacedarbnovmucppbjkcwsxopludrj5ttmtm7mzfqsugmxdnqevqso7o").unwrap(),
    ];
    known_cids.contains(cid)
}

/// Init actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_init_v8::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if is_v8_init_cid(&actor.code) {
            return store
                .get_obj(&actor.state)?
                .map(State::V8)
                .context("Actor state doesn't exist in store");
        }
        Err(anyhow::anyhow!("Unknown init actor code {}", actor.code))
    }

    /// Allocates a new ID address and stores a mapping of the argument address to it.
    /// Returns the newly-allocated address.
    pub fn map_address_to_new_id<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
    ) -> anyhow::Result<Address> {
        match self {
            State::V8(st) => Ok(Address::new_id(st.map_address_to_new_id(&store, addr)?)),
        }
    }

    /// Resolves an address to an ID-address, if possible.
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
    ) -> anyhow::Result<Option<Address>> {
        match self {
            State::V8(st) => st.resolve_address(&store, addr),
        }
    }

    pub fn into_network_name(self) -> String {
        match self {
            State::V8(st) => st.network_name,
        }
    }
}
