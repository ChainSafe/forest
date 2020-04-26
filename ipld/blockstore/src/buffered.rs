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
    /// Flushes the buffered cache based on the root node
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
    fn write_recursive(&self, cid: &Cid) -> Result<(), Box<dyn StdError>> {
        todo!()
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
    use cid::multihash::{Blake2b256, Identity};
    use commcid::{commitment_to_cid, FilecoinMultihashCode};
    use forest_ipld::Ipld;
    use std::collections::BTreeMap;

    #[test]
    fn basic_buffered_store() {
        let mem = db::MemoryDB::default();
        let mut buf_store = BufferedBlockStore::new(&mem);

        let cid = buf_store.put(&8, Identity).unwrap();
        assert_eq!(mem.get::<u8>(&cid).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&cid).unwrap(), Some(8));

        buf_store.flush(&cid).unwrap();
        assert_eq!(buf_store.get::<u8>(&cid).unwrap(), Some(8));
        assert_eq!(mem.get::<u8>(&cid).unwrap(), Some(8));
    }

    #[test]
    fn buffered_store_with_links() {
        let mem = db::MemoryDB::default();
        let mut buf_store = BufferedBlockStore::new(&mem);
        let str_val = "value";
        let value = 8u8;
        let arr_cid = buf_store.put(&(str_val, value), Blake2b256).unwrap();
        let identity_cid = buf_store.put(&0u8, Identity).unwrap();

        // Create map to insert into store
        let mut map: BTreeMap<String, Ipld> = Default::default();
        map.insert("array".to_owned(), Ipld::Link(arr_cid.clone()));
        let sealed_comm_cid = commitment_to_cid(&[7u8; 32], FilecoinMultihashCode::SealedV1);
        map.insert("sealed".to_owned(), Ipld::Link(sealed_comm_cid));
        let unsealed_comm_cid = commitment_to_cid(&[5u8; 32], FilecoinMultihashCode::UnsealedV1);
        map.insert("unsealed".to_owned(), Ipld::Link(unsealed_comm_cid));
        map.insert("identity".to_owned(), Ipld::Link(identity_cid));
        map.insert("value".to_owned(), Ipld::String(str_val.to_owned()));

        // Make sure a block not connected to the root does not get written
        let unconnected = buf_store.put(&27u8, Blake2b256).unwrap();

        let map_cid = buf_store.put(&map, Blake2b256).unwrap();
        assert_eq!(mem.get::<(String, u8)>(&arr_cid).unwrap(), None);
        assert_eq!(mem.get::<Ipld>(&map_cid).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&unconnected).unwrap(), Some(27u8));

        // Flush and assert changes
        buf_store.flush(&map_cid).unwrap();
        assert_eq!(mem.get::<Ipld>(&map_cid).unwrap(), Some(Ipld::Map(map)));
        assert_eq!(
            mem.get::<(String, u8)>(&arr_cid).unwrap(),
            Some((str_val.to_owned(), value))
        );
        // assert_eq!(mem.get::<u8>(&unconnected).unwrap(), None);
        // assert_eq!(buf_store.get::<u8>(&unconnected).unwrap(), None);
    }
}
