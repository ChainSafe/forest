// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ActorVersion;
use anyhow::Error as AnyhowError;
use cid::Cid;
use fil_types::HAMT_BIT_WIDTH;
use forest_hash_utils::{BytesKey, Hash};
use ipld_blockstore::BlockStore;
use ipld_blockstore::FvmRefStore;
use serde::{de::DeserializeOwned, Serialize};
use std::borrow::Borrow;

pub enum Map<'a, BS, V> {
    V7(fil_actors_runtime_v7::fvm_ipld_hamt::Hamt<FvmRefStore<'a, BS>, V, BytesKey>),
}

impl<'a, BS, V> Map<'a, BS, V>
where
    V: Serialize + DeserializeOwned + PartialEq,
    BS: BlockStore,
{
    pub fn new(store: &'a BS, version: ActorVersion) -> Self {
        match version {
            ActorVersion::V7 => Map::V7(
                fil_actors_runtime_v7::fvm_ipld_hamt::Hamt::new_with_bit_width(
                    FvmRefStore::new(store),
                    HAMT_BIT_WIDTH,
                ),
            ),
            _ => panic!("unsupported actor version: {}", version),
        }
    }

    /// Load map with root
    pub fn load(cid: &Cid, store: &'a BS, version: ActorVersion) -> Result<Self, anyhow::Error> {
        match version {
            ActorVersion::V7 => Ok(Map::V7(
                fil_actors_runtime_v7::fvm_ipld_hamt::Hamt::load_with_bit_width(
                    cid,
                    FvmRefStore::new(store),
                    HAMT_BIT_WIDTH,
                )?,
            )),
            _ => panic!("unsupported actor version: {}", version),
        }
    }

    /// Returns a reference to the underlying store of the `Map`.
    pub fn store(&self) -> &'a BS {
        match self {
            Map::V7(m) => m.store().bs,
        }
    }

    /// Inserts a key-value pair into the `Map`.
    pub fn set(&mut self, key: BytesKey, value: V) -> Result<(), AnyhowError> {
        match self {
            Map::V7(m) => {
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
            Map::V7(m) => Ok(m.get(k)?),
        }
    }

    /// Returns `true` if a value exists for the given key in the `Map`.
    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> Result<bool, AnyhowError>
    where
        BytesKey: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self {
            Map::V7(m) => Ok(m.contains_key(k)?),
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
            Map::V7(m) => Ok(m.delete(k)?),
        }
    }

    /// Flush root and return Cid for `Map`
    pub fn flush(&mut self) -> Result<Cid, AnyhowError> {
        match self {
            Map::V7(m) => Ok(m.flush()?),
        }
    }

    /// Iterates over each KV in the `Map` and runs a function on the values.
    pub fn for_each<F>(&self, mut f: F) -> Result<(), anyhow::Error>
    where
        V: DeserializeOwned,
        F: FnMut(&BytesKey, &V) -> Result<(), anyhow::Error>,
    {
        match self {
            Map::V7(m) => m
                .for_each(|key, val| f(key, val).map_err(|e| anyhow::anyhow!("{}", e)))
                .map_err(|e| e.into()),
        }
    }
}
