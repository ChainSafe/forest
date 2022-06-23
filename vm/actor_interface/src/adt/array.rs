// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ActorVersion;
use anyhow::Error as AnyhowError;
use cid::Cid;
use ipld_blockstore::BlockStore;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;

pub enum Array<'a, BS, V> {
    // V0(actorv0::ipld_amt::Amt<'a, V, BS>),
    // V2(actorv2::ipld_amt::Amt<'a, V, BS>),
    // V3(actorv3::ipld_amt::Amt<'a, V, BS>),
    // V4(actorv4::ipld_amt::Amt<'a, V, BS>),
    // V5(actorv5::ipld_amt::Amt<'a, V, BS>),
    _UnusedArray(PhantomData<(&'a BS, V)>),
}

impl<'a, BS, V> Array<'a, BS, V>
where
    V: Serialize + DeserializeOwned,
    BS: BlockStore,
{
    pub fn new(_store: &'a BS, _version: ActorVersion) -> Self {
        panic!("Cannot create Array")
        // match version {
        //     // ActorVersion::V0 => Array::V0(actorv0::ipld_amt::Amt::new(store)),
        //     // ActorVersion::V2 => Array::V2(actorv2::ipld_amt::Amt::new(store)),
        //     // ActorVersion::V3 => Array::V3(actorv3::ipld_amt::Amt::new(store)),
        //     // ActorVersion::V4 => Array::V4(actorv4::ipld_amt::Amt::new(store)),
        //     // ActorVersion::V5 => Array::V5(actorv5::ipld_amt::Amt::new(store)),
        //     // ActorVersion::V6 => Array::V5(actorv5::ipld_amt::Amt::new(store)),
        //     _ => panic!("Cannot create Array"),
        // }
    }

    /// Load map with root
    pub fn load(_cid: &Cid, _store: &'a BS, _version: ActorVersion) -> Result<Self, AnyhowError> {
        panic!("Cannot load Array")
        // match version {
        //     // ActorVersion::V0 => Ok(Array::V0(actorv0::ipld_amt::Amt::load(cid, store)?)),
        //     // ActorVersion::V2 => Ok(Array::V2(actorv2::ipld_amt::Amt::load(cid, store)?)),
        //     // ActorVersion::V3 => Ok(Array::V3(actorv3::ipld_amt::Amt::load(cid, store)?)),
        //     // ActorVersion::V4 => Ok(Array::V4(actorv4::ipld_amt::Amt::load(cid, store)?)),
        //     // ActorVersion::V5 => Ok(Array::V5(actorv5::ipld_amt::Amt::load(cid, store)?)),
        //     // ActorVersion::V6 => Ok(Array::V5(actorv5::ipld_amt::Amt::load(cid, store)?)),
        //     _ => panic!("Cannot load Array"),
        // }
    }

    /// Gets count of elements added in the `Array`.
    pub fn count(&self) -> u64 {
        panic!("Cannot count Array")
        // match self {
        //     // Array::V0(m) => m.count(),
        //     // Array::V2(m) => m.count(),
        //     // Array::V3(m) => m.count() as u64,
        //     // Array::V4(m) => m.count() as u64,
        //     // Array::V5(m) => m.count() as u64,
        //     _ => panic!("Cannot count Array"),
        // }
    }

    /// Get value at index of `Array`
    pub fn get(&self, _i: u64) -> Result<Option<&V>, AnyhowError> {
        panic!("Cannot get Array")
        // match self {
        //     // Array::V0(m) => Ok(m.get(i)?),
        //     // Array::V2(m) => Ok(m.get(i)?),
        //     // Array::V3(m) => Ok(m.get(i as usize)?),
        //     // Array::V4(m) => Ok(m.get(i as usize)?),
        //     // Array::V5(m) => Ok(m.get(i as usize)?),
        //     _ => panic!("Cannot get Array"),
        // }
    }

    /// Set value at index
    pub fn set(&mut self, _i: u64, _val: V) -> Result<(), AnyhowError> {
        unimplemented!()
        // match self {
        //     // Array::V0(m) => Ok(m.set(i, val)?),
        //     // Array::V2(m) => Ok(m.set(i, val)?),
        //     // Array::V3(m) => Ok(m.set(i as usize, val)?),
        //     // Array::V4(m) => Ok(m.set(i as usize, val)?),
        //     // Array::V5(m) => Ok(m.set(i as usize, val)?),
        //     _ => unimplemented!(),
        // }
    }

    /// Delete item from `Array` at index
    pub fn delete(&mut self, _i: u64) -> Result<bool, AnyhowError> {
        unimplemented!()
        // match self {
        //     // Array::V0(m) => Ok(m.delete(i)?),
        //     // Array::V2(m) => Ok(m.delete(i)?),
        //     // Array::V3(m) => Ok(m.delete(i as usize)?.is_some()),
        //     // Array::V4(m) => Ok(m.delete(i as usize)?.is_some()),
        //     // Array::V5(m) => Ok(m.delete(i as usize)?.is_some()),
        //     _ => unimplemented!(),
        // }
    }

    /// flush root and return Cid used as key in block store
    pub fn flush(&mut self) -> Result<Cid, AnyhowError> {
        unimplemented!()
        // match self {
        //     // Array::V0(m) => Ok(m.flush()?),
        //     // Array::V2(m) => Ok(m.flush()?),
        //     // Array::V3(m) => Ok(m.flush()?),
        //     // Array::V4(m) => Ok(m.flush()?),
        //     // Array::V5(m) => Ok(m.flush()?),
        //     _ => unimplemented!(),
        // }
    }

    /// Iterates over each value in the `Array` and runs a function on the values.
    pub fn for_each<F>(&self, _f: F) -> Result<(), AnyhowError>
    where
        F: FnMut(u64, &V) -> Result<(), AnyhowError>,
    {
        unimplemented!()
        // match self {
        //     // Array::V0(m) => m.for_each(f),
        //     // Array::V2(m) => m.for_each(f),
        //     // Array::V3(m) => m.for_each(|k: usize, v: &V| f(k as u64, v)),
        //     // Array::V4(m) => m.for_each(|k: usize, v: &V| f(k as u64, v)),
        //     // Array::V5(m) => m.for_each(|k: usize, v: &V| f(k as u64, v)),
        //     _ => unimplemented!(),
        // }
    }
}
