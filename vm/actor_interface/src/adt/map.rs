// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ActorVersion;
use cid::Cid;
use fil_types::HAMT_BIT_WIDTH;
use forest_hash_utils::{BytesKey, Hash};
use ipld_blockstore::BlockStore;
use serde::{de::DeserializeOwned, Serialize};
use std::borrow::Borrow;
use std::error::Error;
pub enum Map<'a, BS, V> {
    V0(actorv0::Map<'a, BS, V>),
    V2(actorv2::Map<'a, BS, V>),
    V3(actorv3::Map<'a, BS, V>),
    V4(actorv4::Map<'a, BS, V>),
}

impl<'a, BS, V> Map<'a, BS, V>
where
    V: Serialize + DeserializeOwned + PartialEq,
    BS: BlockStore,
{
    pub fn new(store: &'a BS, version: ActorVersion) -> Self {
        match version {
            ActorVersion::V0 => Map::V0(actorv0::make_map(store)),
            ActorVersion::V2 => Map::V2(actorv2::make_map(store)),
            ActorVersion::V3 => Map::V3(actorv3::make_empty_map(store, HAMT_BIT_WIDTH)),
            ActorVersion::V4 => Map::V4(actorv4::make_empty_map(store, HAMT_BIT_WIDTH)),
        }
    }

    /// Load map with root
    pub fn load(cid: &Cid, store: &'a BS, version: ActorVersion) -> Result<Self, Box<dyn Error>> {
        match version {
            ActorVersion::V0 => Ok(Map::V0(actorv0::make_map_with_root(cid, store)?)),
            ActorVersion::V2 => Ok(Map::V2(actorv2::make_map_with_root(cid, store)?)),
            ActorVersion::V3 => Ok(Map::V3(actorv3::make_map_with_root(cid, store)?)),
            ActorVersion::V4 => Ok(Map::V4(actorv4::make_map_with_root(cid, store)?)),
        }
    }

    /// Returns a reference to the underlying store of the `Map`.
    pub fn store(&self) -> &'a BS {
        match self {
            Map::V0(m) => m.store(),
            Map::V2(m) => m.store(),
            Map::V3(m) => m.store(),
            Map::V4(m) => m.store(),
        }
    }

    /// Inserts a key-value pair into the `Map`.
    pub fn set(&mut self, key: BytesKey, value: V) -> Result<(), Box<dyn Error>> {
        match self {
            Map::V0(m) => Ok(m.set(key, value)?),
            Map::V2(m) => Ok(m.set(key, value)?),
            Map::V3(m) => {
                m.set(key, value)?;
                Ok(())
            }
            Map::V4(m) => {
                m.set(key, value)?;
                Ok(())
            }
        }
    }

    /// Returns a reference to the value corresponding to the key.
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Result<Option<&V>, Box<dyn Error>>
    where
        BytesKey: Borrow<Q>,
        Q: Hash + Eq,
        V: DeserializeOwned,
    {
        match self {
            Map::V0(m) => Ok(m.get(k)?),
            Map::V2(m) => Ok(m.get(k)?),
            Map::V3(m) => Ok(m.get(k)?),
            Map::V4(m) => Ok(m.get(k)?),
        }
    }

    /// Returns `true` if a value exists for the given key in the `Map`.
    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> Result<bool, Box<dyn Error>>
    where
        BytesKey: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self {
            Map::V0(m) => Ok(m.contains_key(k)?),
            Map::V2(m) => Ok(m.contains_key(k)?),
            Map::V3(m) => Ok(m.contains_key(k)?),
            Map::V4(m) => Ok(m.contains_key(k)?),
        }
    }

    /// Removes a key from the `Map`, returning the value at the key if the key
    /// was previously in the `Map`.
    pub fn delete<Q: ?Sized>(&mut self, k: &Q) -> Result<Option<(BytesKey, V)>, Box<dyn Error>>
    where
        BytesKey: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self {
            Map::V0(m) => Ok(m.delete(k)?),
            Map::V2(m) => Ok(m.delete(k)?),
            Map::V3(m) => Ok(m.delete(k)?),
            Map::V4(m) => Ok(m.delete(k)?),
        }
    }

    /// Flush root and return Cid for `Map`
    pub fn flush(&mut self) -> Result<Cid, Box<dyn Error>> {
        match self {
            Map::V0(m) => Ok(m.flush()?),
            Map::V2(m) => Ok(m.flush()?),
            Map::V3(m) => Ok(m.flush()?),
            Map::V4(m) => Ok(m.flush()?),
        }
    }

    /// Iterates over each KV in the `Map` and runs a function on the values.
    pub fn for_each<F>(&self, f: F) -> Result<(), Box<dyn Error>>
    where
        V: DeserializeOwned,
        F: FnMut(&BytesKey, &V) -> Result<(), Box<dyn Error>>,
    {
        match self {
            Map::V0(m) => m.for_each(f),
            Map::V2(m) => m.for_each(f),
            Map::V3(m) => m.for_each(f),
            Map::V4(m) => m.for_each(f),
        }
    }
}
