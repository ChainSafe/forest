// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockStore;
use cid::{multihash::MultihashDigest, Cid};
use db::{Error, Store};
use encoding::{ser::Serialize, to_vec};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error as StdError;

/// Wrapper around `BlockStore` to limit and have control over when values are written.
/// This type is not threadsafe and can only be used in synchronous contexts.
#[derive(Debug)]
pub struct BufferedBlockStore<'bs, BS> {
    base: &'bs BS,
    write: RefCell<HashMap<Cid, Vec<u8>>>,
}

impl<'bs, BS> BufferedBlockStore<'bs, BS>
where
    BS: BlockStore,
{
    pub fn new(base: &'bs BS) -> Self {
        Self {
            base,
            write: Default::default(),
        }
    }
    pub fn flush(&mut self, _root: &Cid) -> Result<(), Box<dyn StdError>> {
        // TODO update this to only write over values connected to the root
        // This will be done by querying root in `self.write` and writing all values connected
        // through links from that root that doesn't exist in `self.base`
        for (k, v) in self.write.borrow().iter() {
            self.base.write(k.to_bytes(), v)?;
        }

        self.write = Default::default();
        Ok(())
    }
}

impl<BS> BlockStore for BufferedBlockStore<'_, BS>
where
    BS: BlockStore,
{
    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Box<dyn StdError>> {
        if let Some(data) = self.write.borrow().get(cid) {
            return Ok(Some(data.clone()));
        }

        self.base.get_bytes(cid)
    }

    fn put<S, T>(&self, obj: &S, hash: T) -> Result<Cid, Box<dyn StdError>>
    where
        S: Serialize,
        T: MultihashDigest,
    {
        let bz = to_vec(obj)?;
        let cid = Cid::new_from_cbor(&bz, hash);
        self.write.borrow_mut().insert(cid.clone(), bz);
        Ok(cid)
    }
}

impl<BS> Store for BufferedBlockStore<'_, BS>
where
    BS: Store,
{
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.read(key)
    }
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.base.write(key, value)
    }
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.delete(key)
    }
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.exists(key)
    }
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.bulk_read(keys)
    }
    fn bulk_write<K, V>(&self, keys: &[K], values: &[V]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.base.bulk_write(keys, values)
    }
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.bulk_delete(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::multihash::Identity;

    #[test]
    fn basic_buffered_store() {
        let mem = db::MemoryDB::default();
        let buf_store = BufferedBlockStore::new(&mem);

        let cid = buf_store.put(&8, Identity).unwrap();
        assert_eq!(mem.get::<u8>(&cid).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&cid).unwrap(), Some(8));
    }
}
