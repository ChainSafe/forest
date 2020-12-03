// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "buffered")]

use super::BlockStore;
use cid::{Cid, Code, DAG_CBOR};
use db::{Error, Store};
use encoding::from_slice;
use forest_ipld::Ipld;
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
    /// Flushes the buffered cache based on the root node.
    /// This will recursively traverse the cache and write all data connected by links to this
    /// root Cid.
    pub fn flush(&mut self, root: &Cid) -> Result<(), Box<dyn StdError>> {
        write_recursive(self.base, &self.write.borrow(), root)?;

        self.write = Default::default();
        Ok(())
    }
}

/// Recursively traverses cache through Cid links.
fn write_recursive<BS>(
    base: &BS,
    cache: &HashMap<Cid, Vec<u8>>,
    cid: &Cid,
) -> Result<(), Box<dyn StdError>>
where
    BS: BlockStore,
{
    // Skip identity and Filecoin commitment Cids
    if cid.codec() != DAG_CBOR {
        return Ok(());
    }

    let raw_cid_bz = cid.to_bytes();

    // If root exists in base store already, can skip
    if base.exists(&raw_cid_bz)? {
        return Ok(());
    }

    let raw_bz = cache
        .get(cid)
        .ok_or_else(|| format!("Invalid link ({}) in flushing buffered store", cid))?;

    // Deserialize the bytes to Ipld to traverse links.
    // This is safer than finding links in place,
    // but slightly slower to copy and potentially allocate non Cid data.
    let block: Ipld = from_slice(raw_bz)?;

    // Traverse and write linked data recursively
    for_each_link(&block, &|c| write_recursive(base, cache, c))?;

    // Write the root node to base storage
    base.write(&raw_cid_bz, raw_bz)?;
    Ok(())
}

/// Recursively explores Ipld for links and calls a function with a reference to the Cid.
fn for_each_link<F>(ipld: &Ipld, cb: &F) -> Result<(), Box<dyn StdError>>
where
    F: Fn(&Cid) -> Result<(), Box<dyn StdError>>,
{
    match ipld {
        Ipld::Link(c) => cb(&c)?,
        Ipld::List(arr) => {
            for item in arr {
                for_each_link(item, cb)?
            }
        }
        Ipld::Map(map) => {
            for v in map.values() {
                for_each_link(v, cb)?
            }
        }
        _ => (),
    }
    Ok(())
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

    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let cid = cid::new_from_cbor(&bytes, code);
        self.write.borrow_mut().insert(cid, bytes);
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
    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.base.bulk_write(values)
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
    use cid::{multihash::MultihashDigest, Code, RAW};
    use commcid::commitment_to_cid;
    use forest_ipld::{ipld, Ipld};

    #[test]
    fn basic_buffered_store() {
        let mem = db::MemoryDB::default();
        let mut buf_store = BufferedBlockStore::new(&mem);

        let cid = buf_store.put(&8, Code::Blake2b256).unwrap();
        assert_eq!(mem.get::<u8>(&cid).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&cid).unwrap(), Some(8));

        buf_store.flush(&cid).unwrap();
        assert_eq!(buf_store.get::<u8>(&cid).unwrap(), Some(8));
        assert_eq!(mem.get::<u8>(&cid).unwrap(), Some(8));
        assert_eq!(buf_store.write.borrow().get(&cid), None);
    }

    #[test]
    fn buffered_store_with_links() {
        let mem = db::MemoryDB::default();
        let mut buf_store = BufferedBlockStore::new(&mem);
        let str_val = "value";
        let value = 8u8;
        let arr_cid = buf_store.put(&(str_val, value), Code::Blake2b256).unwrap();
        let identity_cid = Cid::new_v1(RAW, Code::Identity.digest(&[0u8]));

        // Create map to insert into store
        let sealed_comm_cid = commitment_to_cid(
            cid::FIL_COMMITMENT_SEALED,
            cid::POSEIDON_BLS12_381_A1_FC1,
            &[7u8; 32],
        )
        .unwrap();
        let unsealed_comm_cid = commitment_to_cid(
            cid::FIL_COMMITMENT_UNSEALED,
            cid::SHA2_256_TRUNC254_PADDED,
            &[5u8; 32],
        )
        .unwrap();
        let map = ipld!({
            "array": Link(arr_cid.clone()),
            "sealed": Link(sealed_comm_cid.clone()),
            "unsealed": Link(unsealed_comm_cid.clone()),
            "identity": Link(identity_cid.clone()),
            "value": str_val,
        });
        let map_cid = buf_store.put(&map, Code::Blake2b256).unwrap();

        let root_cid = buf_store
            .put(&(map_cid.clone(), 1u8), Code::Blake2b256)
            .unwrap();

        // Make sure a block not connected to the root does not get written
        let unconnected = buf_store.put(&27u8, Code::Blake2b256).unwrap();

        assert_eq!(mem.get::<Ipld>(&map_cid).unwrap(), None);
        assert_eq!(mem.get::<Ipld>(&root_cid).unwrap(), None);
        assert_eq!(mem.get::<(String, u8)>(&arr_cid).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&unconnected).unwrap(), Some(27u8));

        // Flush and assert changes
        buf_store.flush(&root_cid).unwrap();
        assert_eq!(
            mem.get::<(String, u8)>(&arr_cid).unwrap(),
            Some((str_val.to_owned(), value))
        );
        assert_eq!(mem.get::<Ipld>(&map_cid).unwrap(), Some(map));
        assert_eq!(
            mem.get::<Ipld>(&root_cid).unwrap(),
            Some(ipld!([Link(map_cid), 1]))
        );
        assert_eq!(buf_store.get::<u8>(&identity_cid).unwrap(), None);
        assert_eq!(buf_store.get::<Ipld>(&unsealed_comm_cid).unwrap(), None);
        assert_eq!(buf_store.get::<Ipld>(&sealed_comm_cid).unwrap(), None);
        assert_eq!(mem.get::<u8>(&unconnected).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&unconnected).unwrap(), None);
    }
}
