// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};

use super::*;
use crate::*;

impl Blockstore for RollingDB {
    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        for db in self.dbs.iter() {
            if let Ok(true) = Blockstore::has(db, k) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        for db in self.dbs.iter() {
            if let Ok(Some(v)) = Blockstore::get(db, k) {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    fn put<D>(
        &self,
        mh_code: cid::multihash::Code,
        block: &fvm_ipld_blockstore::Block<D>,
    ) -> anyhow::Result<Cid>
    where
        Self: Sized,
        D: AsRef<[u8]>,
    {
        Blockstore::put(self.current(), mh_code, block)
    }

    fn put_many<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (cid::multihash::Code, fvm_ipld_blockstore::Block<D>)>,
    {
        Blockstore::put_many(self.current(), blocks)
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        Blockstore::put_many_keyed(self.current(), blocks)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        Blockstore::put_keyed(self.current(), k, block)
    }
}

impl Store for RollingDB {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        for db in self.dbs.iter() {
            if let Ok(Some(v)) = Store::read(db, key.as_ref()) {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    fn exists<K>(&self, key: K) -> Result<bool, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        for db in self.dbs.iter() {
            if let Ok(true) = Store::exists(db, key.as_ref()) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), crate::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Store::write(self.current(), key, value)
    }

    fn delete<K>(&self, key: K) -> Result<(), crate::Error>
    where
        K: AsRef<[u8]>,
    {
        Store::delete(self.current(), key)
    }

    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), crate::Error> {
        Store::bulk_write(self.current(), values)
    }

    fn flush(&self) -> Result<(), crate::Error> {
        Store::flush(self.current())
    }
}

impl BitswapStoreRead for RollingDB {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        for db in self.dbs.iter() {
            if let Ok(true) = BitswapStoreRead::contains(db, cid) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        for db in self.dbs.iter() {
            if let Ok(Some(v)) = BitswapStoreRead::get(db, cid) {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }
}

impl BitswapStoreReadWrite for RollingDB {
    type Params = <Db as BitswapStoreReadWrite>::Params;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        BitswapStoreReadWrite::insert(self.current(), block)
    }
}

impl DBStatistics for RollingDB {
    fn get_statistics(&self) -> Option<String> {
        DBStatistics::get_statistics(self.current())
    }
}
