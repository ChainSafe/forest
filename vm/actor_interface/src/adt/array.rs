// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ActorVersion;
use cid::Cid;
use ipld_blockstore::BlockStore;
use serde::{de::DeserializeOwned, Serialize};
use std::error::Error;

pub enum Array<'a, BS, V> {
    V0(actorv0::ipld_amt::Amt<'a, V, BS>),
    V2(actorv2::ipld_amt::Amt<'a, V, BS>),
    V3(actorv3::ipld_amt::Amt<'a, V, BS>),
}

impl<'a, BS, V> Array<'a, BS, V>
where
    V: Serialize + DeserializeOwned,
    BS: BlockStore,
{
    pub fn new(store: &'a BS, version: ActorVersion) -> Self {
        match version {
            ActorVersion::V0 => Array::V0(actorv0::ipld_amt::Amt::new(store)),
            ActorVersion::V2 => Array::V2(actorv2::ipld_amt::Amt::new(store)),
            ActorVersion::V3 => Array::V3(actorv3::ipld_amt::Amt::new(store)),
        }
    }

    /// Load map with root
    pub fn load(cid: &Cid, store: &'a BS, version: ActorVersion) -> Result<Self, Box<dyn Error>> {
        match version {
            ActorVersion::V0 => Ok(Array::V0(actorv0::ipld_amt::Amt::load(cid, store)?)),
            ActorVersion::V2 => Ok(Array::V2(actorv2::ipld_amt::Amt::load(cid, store)?)),
            ActorVersion::V3 => Ok(Array::V3(actorv3::ipld_amt::Amt::load(cid, store)?)),
        }
    }

    /// Gets count of elements added in the `Array`.
    pub fn count(&self) -> u64 {
        match self {
            Array::V0(m) => m.count(),
            Array::V2(m) => m.count(),
            Array::V3(m) => m.count() as u64,
        }
    }

    /// Get value at index of `Array`
    pub fn get(&self, i: u64) -> Result<Option<&V>, Box<dyn Error>> {
        match self {
            Array::V0(m) => Ok(m.get(i)?),
            Array::V2(m) => Ok(m.get(i)?),
            Array::V3(m) => Ok(m.get(i as usize)?),
        }
    }

    /// Set value at index
    pub fn set(&mut self, i: u64, val: V) -> Result<(), Box<dyn Error>> {
        match self {
            Array::V0(m) => Ok(m.set(i, val)?),
            Array::V2(m) => Ok(m.set(i, val)?),
            Array::V3(m) => Ok(m.set(i as usize, val)?),
        }
    }

    /// Delete item from `Array` at index
    pub fn delete(&mut self, i: u64) -> Result<bool, Box<dyn Error>> {
        match self {
            Array::V0(m) => Ok(m.delete(i)?),
            Array::V2(m) => Ok(m.delete(i)?),
            Array::V3(m) => Ok(m.delete(i as usize)?.is_some()),
        }
    }

    /// flush root and return Cid used as key in block store
    pub fn flush(&mut self) -> Result<Cid, Box<dyn Error>> {
        match self {
            Array::V0(m) => Ok(m.flush()?),
            Array::V2(m) => Ok(m.flush()?),
            Array::V3(m) => Ok(m.flush()?),
        }
    }

    /// Iterates over each value in the `Array` and runs a function on the values.
    pub fn for_each<F>(&self, mut f: F) -> Result<(), Box<dyn Error>>
    where
        F: FnMut(u64, &V) -> Result<(), Box<dyn Error>>,
    {
        match self {
            Array::V0(m) => m.for_each(f),
            Array::V2(m) => m.for_each(f),
            Array::V3(m) => m.for_each(|k: usize, v: &V| f(k as u64, v)),
        }
    }
}
