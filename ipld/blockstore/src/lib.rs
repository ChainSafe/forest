// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash::MultihashDigest, Cid};
use db::{Error, MemoryDB, RocksDb, Store};
use encoding::{de::DeserializeOwned, from_slice, ser::Serialize, to_vec};

/// Wrapper for database to handle inserting and retrieving ipld data with Cids
pub trait BlockStore: Store {
    /// Get bytes from block store by Cid
    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.read(cid.to_bytes())?)
    }

    /// Get typed object from block store by Cid
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned,
    {
        match self.get_bytes(cid)? {
            Some(bz) => Ok(Some(from_slice(&bz)?)),
            None => Ok(None),
        }
    }

    /// Put an object in the block store and return the Cid identifier
    fn put<S, T>(&self, obj: &S, hash: T) -> Result<Cid, Error>
    where
        S: Serialize,
        T: MultihashDigest,
    {
        let bz = to_vec(obj)?;
        let cid = Cid::new_from_cbor(&bz, hash).map_err(|e| Error::Encoding(e.to_string()))?;
        self.write(cid.to_bytes(), bz)?;
        Ok(cid)
    }
}

impl BlockStore for MemoryDB {}
impl BlockStore for RocksDb {}
