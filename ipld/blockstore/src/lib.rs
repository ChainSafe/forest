// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(feature = "buffered")]
mod buffered;
#[cfg(feature = "resolve")]
/// This module is used for resolving Cids and Ipld recursively. This is generally only needed
/// for testing because links should generally not be collapsed to generate a singular data
/// structure, or this would lead to ambiguity of the data.
pub mod resolve;
#[cfg(feature = "sled")]
mod sled;
#[cfg(feature = "tracking")]
mod tracking;

#[cfg(feature = "buffered")]
pub use self::buffered::BufferedBlockStore;

#[cfg(feature = "tracking")]
pub use self::tracking::{BSStats, TrackingBlockStore};

use cid::{Cid, Code};
use db::{MemoryDB, Store};
use encoding::{de::DeserializeOwned, from_slice, ser::Serialize, to_vec};
use std::error::Error as StdError;

#[cfg(feature = "rocksdb")]
use db::rocks::{RocksDb, WriteBatch};

/// Wrapper for database to handle inserting and retrieving ipld data with Cids
pub trait BlockStore: Store {
    /// Get bytes from block store by Cid.
    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Box<dyn StdError>> {
        Ok(self.read(cid.to_bytes())?)
    }

    /// Get typed object from block store by Cid.
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, Box<dyn StdError>>
    where
        T: DeserializeOwned,
    {
        match self.get_bytes(cid)? {
            Some(bz) => Ok(Some(from_slice(&bz)?)),
            None => Ok(None),
        }
    }

    /// Put an object in the block store and return the Cid identifier.
    fn put<S>(&self, obj: &S, code: Code) -> Result<Cid, Box<dyn StdError>>
    where
        S: Serialize,
    {
        let bytes = to_vec(obj)?;
        self.put_raw(bytes, code)
    }

    /// Put raw bytes in the block store and return the Cid identifier.
    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let cid = cid::new_from_cbor(&bytes, code);
        self.write(cid.to_bytes(), bytes)?;
        Ok(cid)
    }

    /// Batch put cbor objects into blockstore and returns vector of Cids
    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> Result<Vec<Cid>, Box<dyn StdError>>
    where
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        values
            .into_iter()
            .map(|value| self.put(value, code))
            .collect()
    }
}

impl BlockStore for MemoryDB {}

#[cfg(feature = "rocksdb")]
impl BlockStore for RocksDb {
    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> Result<Vec<Cid>, Box<dyn StdError>>
    where
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        let mut batch = WriteBatch::default();
        let cids: Vec<Cid> = values
            .into_iter()
            .map(|v| {
                let bz = to_vec(v)?;
                let cid = cid::new_from_cbor(&bz, code);
                batch.put(cid.to_bytes(), bz);
                Ok(cid)
            })
            .collect::<Result<_, Box<dyn StdError>>>()?;
        self.db.write(batch)?;

        Ok(cids)
    }
}
