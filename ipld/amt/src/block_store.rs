// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Error;
use cid::Cid;
use db::{MemoryDB, Read, RocksDb, Write};
use encoding::{de::DeserializeOwned, from_slice, ser::Serialize, to_vec};

/// Wrapper for database to handle inserting and retrieving data from AMT with Cids
pub trait BlockStore: Read + Write {
    /// Get bytes from block store by Cid
    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.read(cid.to_bytes())?)
    }

    /// Get typed object from block store by Cid
    fn get_typed<T>(&self, cid: &Cid) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned,
    {
        match self.get(cid)? {
            Some(bz) => Ok(Some(from_slice(&bz)?)),
            None => Ok(None),
        }
    }

    /// Put an object in the block store and return the Cid identifier
    fn put<S>(&self, obj: &S) -> Result<Cid, Error>
    where
        S: Serialize,
    {
        let bz = to_vec(obj)?;
        let cid = Cid::from_bytes_default(&bz)?;
        self.write(cid.to_bytes(), bz)?;
        Ok(cid)
    }
}

impl BlockStore for MemoryDB {}
impl BlockStore for RocksDb {}
