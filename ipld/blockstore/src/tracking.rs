// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "tracking")]

use super::{BlockStore, BlockStoreExt};
use cid::Cid;
use db::{Error, Store};
use fvm_ipld_blockstore::Blockstore;

use std::cell::RefCell;

/// Stats for a [TrackingBlockStore] this indicates the amount of read and written data
/// to the wrapped store.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BSStats {
    /// Number of reads
    pub r: usize,
    /// Number of writes
    pub w: usize,
    /// Bytes Read
    pub br: usize,
    /// Bytes Written
    pub bw: usize,
}

/// Wrapper around `BlockStore` to tracking reads and writes for verification.
/// This struct should only be used for testing.
#[derive(Debug)]
pub struct TrackingBlockStore<'bs, BS> {
    base: &'bs BS,
    pub stats: RefCell<BSStats>,
}

impl<'bs, BS> TrackingBlockStore<'bs, BS>
where
    BS: BlockStore,
{
    pub fn new(base: &'bs BS) -> Self {
        Self {
            base,
            stats: Default::default(),
        }
    }
}

impl<BS: Blockstore + BlockStoreExt> Blockstore for TrackingBlockStore<'_, BS> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.stats.borrow_mut().r += 1;
        let bytes = self.base.get_bytes_anyhow(k)?;
        if let Some(bytes) = &bytes {
            self.stats.borrow_mut().br += bytes.len();
        }
        Ok(bytes)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.stats.borrow_mut().w += 1;
        self.stats.borrow_mut().bw += block.len();
        self.write(k.to_bytes(), block).map_err(|e| e.into())
    }
}

impl<BS> Store for TrackingBlockStore<'_, BS>
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
    use cid::Code::Blake2b256;

    #[test]
    fn basic_tracking_store() {
        let mem = db::MemoryDB::default();
        let tr_store = TrackingBlockStore::new(&mem);
        assert_eq!(*tr_store.stats.borrow(), BSStats::default());

        type TestType = (u8, String);
        let object: TestType = (8, "test".to_string());
        let obj_bytes_len = encoding::to_vec(&object).unwrap().len();

        tr_store
            .get_obj::<u8>(&cid::new_from_cbor(&[0], Blake2b256))
            .unwrap();
        assert_eq!(
            *tr_store.stats.borrow(),
            BSStats {
                r: 1,
                ..Default::default()
            }
        );

        let put_cid = tr_store.put_obj(&object, Blake2b256).unwrap();
        assert_eq!(
            tr_store.get_obj::<TestType>(&put_cid).unwrap(),
            Some(object)
        );
        assert_eq!(
            *tr_store.stats.borrow(),
            BSStats {
                r: 2,
                br: obj_bytes_len,
                w: 1,
                bw: obj_bytes_len,
            }
        );
    }
}
