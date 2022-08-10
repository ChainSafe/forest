// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{Code, MultihashDigest},
    Cid,
};
use forest_db::Store;
use forest_encoding::{de::DeserializeOwned, ser::Serialize};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{from_slice, to_vec, DAG_CBOR};
use std::sync::Arc;

pub trait BlockStore: Blockstore + Store {}
impl<T: Blockstore + Store> BlockStore for T {}

/// Extension methods for inserting and retrieving ipld data with Cids
pub trait BlockStoreExt: BlockStore {
    /// Get bytes from block store by Cid.
    fn get_bytes(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.get(cid)
    }

    /// Get typed object from block store by Cid
    fn get_obj<T>(&self, cid: &Cid) -> anyhow::Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        match self.get_bytes(cid)? {
            Some(bz) => Ok(Some(from_slice(&bz)?)),
            None => Ok(None),
        }
    }

    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.get(cid)?.is_some())
    }

    /// Put an object in the block store and return the Cid identifier.
    fn put_obj<S>(&self, obj: &S, code: Code) -> anyhow::Result<Cid>
    where
        S: Serialize,
    {
        let bytes = to_vec(obj)?;
        self.put_raw(bytes, code)
    }

    /// Put raw bytes in the block store and return the Cid identifier.
    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> anyhow::Result<Cid> {
        let cid = Cid::new_v1(DAG_CBOR, code.digest(&bytes));
        self.put_keyed(&cid, &bytes)?;
        Ok(cid)
    }

    /// Batch put cbor objects into blockstore and returns vector of Cids
    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> anyhow::Result<Vec<Cid>>
    where
        Self: Sized,
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        let keyed_objects = values
            .into_iter()
            .map(|value| {
                let bytes = to_vec(value)?;
                let cid = Cid::new_v1(DAG_CBOR, code.digest(&bytes));
                Ok((cid, bytes))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let cids = keyed_objects
            .iter()
            .map(|(cid, _)| cid.to_owned())
            .collect();

        self.put_many_keyed(keyed_objects)?;

        Ok(cids)
    }
}

impl<T: BlockStore> BlockStoreExt for T {}

pub struct FvmStore<T> {
    bs: Arc<T>,
}

impl<T> FvmStore<T> {
    pub fn new(bs: Arc<T>) -> Self {
        FvmStore { bs }
    }
}

impl<T: BlockStore> Blockstore for FvmStore<T> {
    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match self.bs.get_bytes(cid) {
            Ok(vs) => Ok(vs),
            Err(_err) => Err(anyhow::Error::msg("Fix FVM error handling")),
        }
    }
    fn put_keyed(&self, cid: &Cid, bytes: &[u8]) -> Result<(), anyhow::Error> {
        self.bs.write(cid.to_bytes(), bytes).map_err(|e| e.into())
    }
}

pub struct FvmRefStore<'a, T> {
    pub bs: &'a T,
}

impl<'a, T> FvmRefStore<'a, T> {
    pub fn new(bs: &'a T) -> Self {
        FvmRefStore { bs }
    }
}

impl<'a, T: BlockStore> Blockstore for FvmRefStore<'a, T> {
    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match self.bs.get_bytes(cid) {
            Ok(vs) => Ok(vs),
            Err(_err) => Err(anyhow::Error::msg("Fix FVM error handling")),
        }
    }
    fn put_keyed(&self, cid: &Cid, bytes: &[u8]) -> Result<(), anyhow::Error> {
        self.bs.write(cid.to_bytes(), bytes).map_err(|e| e.into())
    }
}
