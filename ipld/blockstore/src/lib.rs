// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash::Blake2b256, Cid};
use db::{Error, MemoryDB, Read, RocksDb, Write};
use encoding::{de::DeserializeOwned, from_slice, ser::Serialize, to_vec};

/// Wrapper for database to handle inserting and retrieving data from AMT with Cids
pub trait BlockStore: Read + Write {
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
            Some(bz) => Ok(Some(
                from_slice(&bz).map_err(|e| Error::new(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    // TODO allow put function to set hash type of Cid with multihash::MultihashDigest trait
    /// Put an object in the block store and return the Cid identifier
    fn put<S>(&self, obj: &S) -> Result<Cid, Error>
    where
        S: Serialize,
    {
        let bz = to_vec(obj).map_err(|e| Error::new(e.to_string()))?;
        let cid = Cid::new_from_cbor(&bz, Blake2b256).map_err(|e| Error::new(e.to_string()))?;
        self.write(cid.to_bytes(), bz)?;
        Ok(cid)
    }
}

impl BlockStore for MemoryDB {}
impl BlockStore for RocksDb {}
