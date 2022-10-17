// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ActorVersion;
use anyhow::Error as AnyhowError;
use cid::Cid;
use forest_ipld_blockstore::BlockStore;
use fvm_ipld_hamt::{BytesKey, Hash};
use fvm_shared::HAMT_BIT_WIDTH;
use serde::{de::DeserializeOwned, Serialize};
use std::borrow::Borrow;

pub enum Map<BS, V> {
    V8(fil_actors_runtime_v8::fvm_ipld_hamt::Hamt<BS, V, BytesKey>),
}

impl<BS, V> Map<BS, V>
where
    V: Serialize + DeserializeOwned + PartialEq,
    BS: BlockStore,
{
    pub fn new(store: &BS, version: ActorVersion) -> Self {
        match version {
            ActorVersion::V8 => Map::V8(
                fil_actors_runtime_v8::fvm_ipld_hamt::Hamt::new_with_bit_width(
                    store.clone(),
                    HAMT_BIT_WIDTH,
                ),
            ),
            _ => panic!("unsupported actor version: {}", version),
        }
    }

    /// Load map with root
    pub fn load(cid: &Cid, store: &BS, version: ActorVersion) -> Result<Self, anyhow::Error> {
        match version {
            ActorVersion::V8 => Ok(Map::V8(
                fil_actors_runtime_v8::fvm_ipld_hamt::Hamt::load_with_bit_width(
                    cid,
                    store.clone(),
                    HAMT_BIT_WIDTH,
                )?,
            )),
            _ => panic!("unsupported actor version: {}", version),
        }
    }

    /// Returns a reference to the underlying store of the `Map`.
    pub fn store(&self) -> &BS {
        match self {
            Map::V8(m) => m.store(),
        }
    }

    /// Inserts a key-value pair into the `Map`.
    pub fn set(&mut self, key: BytesKey, value: V) -> Result<(), AnyhowError> {
        match self {
            Map::V8(m) => {
                m.set(key, value)?;
                Ok(())
            }
        }
    }

    /// Returns a reference to the value corresponding to the key.
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Result<Option<&V>, AnyhowError>
    where
        BytesKey: Borrow<Q>,
        Q: Hash + Eq,
        V: DeserializeOwned,
    {
        match self {
            Map::V8(m) => Ok(m.get(k)?),
        }
    }

    /// Returns `true` if a value exists for the given key in the `Map`.
    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> Result<bool, AnyhowError>
    where
        BytesKey: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self {
            Map::V8(m) => Ok(m.contains_key(k)?),
        }
    }

    /// Removes a key from the `Map`, returning the value at the key if the key
    /// was previously in the `Map`.
    pub fn delete<Q: ?Sized>(&mut self, k: &Q) -> Result<Option<(BytesKey, V)>, AnyhowError>
    where
        BytesKey: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self {
            Map::V8(m) => Ok(m.delete(k)?),
        }
    }

    /// Flush root and return Cid for `Map`
    pub fn flush(&mut self) -> Result<Cid, AnyhowError> {
        match self {
            Map::V8(m) => Ok(m.flush()?),
        }
    }

    /// Iterates over each KV in the `Map` and runs a function on the values.
    pub fn for_each<F>(&self, mut f: F) -> Result<(), anyhow::Error>
    where
        V: DeserializeOwned,
        F: FnMut(&BytesKey, &V) -> Result<(), anyhow::Error>,
    {
        match self {
            Map::V8(m) => m
                .for_each(|key, val| f(key, val).map_err(|e| anyhow::anyhow!("{}", e)))
                .map_err(|e| e.into()),
        }
    }
}
