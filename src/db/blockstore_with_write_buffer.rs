// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::{HashMap, HashMapExt};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use parking_lot::RwLock;

pub struct BlockstoreWithWriteBuffer<DB: Blockstore> {
    inner: DB,
    buffer: RwLock<HashMap<Cid, Vec<u8>>>,
    buffer_capacity: usize,
}

impl<DB: Blockstore> Blockstore for BlockstoreWithWriteBuffer<DB> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(v) = self.buffer.read().get(k) {
            return Ok(Some(v.clone()));
        }
        self.inner.get(k)
    }

    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        Ok(self.buffer.read().contains_key(k) || self.inner.has(k)?)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        {
            let mut buffer = self.buffer.write();
            buffer.insert(*k, block.to_vec());
        }
        self.flush_buffer_if_needed()
    }
}

impl<DB: Blockstore> BlockstoreWithWriteBuffer<DB> {
    pub fn new_with_capacity(inner: DB, buffer_capacity: usize) -> Self {
        Self {
            inner,
            buffer_capacity,
            buffer: RwLock::new(HashMap::with_capacity(buffer_capacity)),
        }
    }

    fn flush_buffer(&self) -> anyhow::Result<()> {
        let records = {
            let mut buffer = self.buffer.write();
            buffer.drain().collect_vec()
        };
        self.inner.put_many_keyed(records)
    }

    fn flush_buffer_if_needed(&self) -> anyhow::Result<()> {
        if self.buffer.read().len() >= self.buffer_capacity {
            self.flush_buffer()
        } else {
            Ok(())
        }
    }
}

impl<DB: Blockstore> Drop for BlockstoreWithWriteBuffer<DB> {
    fn drop(&mut self) {
        if let Err(e) = self.flush_buffer() {
            tracing::warn!("{e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::MemoryDB, utils::rand::forest_rng};
    use fvm_ipld_encoding::DAG_CBOR;
    use multihash_codetable::Code::Blake2b256;
    use multihash_codetable::MultihashDigest as _;
    use rand::Rng as _;
    use std::sync::Arc;

    #[test]
    fn test_buffer_flush() {
        const BUFFER_SIZE: usize = 10;
        const N_RECORDS: usize = 15;
        let mem_db = Arc::new(MemoryDB::default());
        let buf_db = BlockstoreWithWriteBuffer::new_with_capacity(mem_db.clone(), BUFFER_SIZE);
        let mut records = Vec::with_capacity(N_RECORDS);
        for _ in 0..N_RECORDS {
            let mut record = [0; 1024];
            forest_rng().fill(&mut record);
            let key = Cid::new_v1(DAG_CBOR, Blake2b256.digest(record.as_slice()));
            records.push((key, record));
        }

        buf_db.put_many_keyed(records.clone()).unwrap();

        for (i, (k, v)) in records.iter().enumerate() {
            assert!(buf_db.has(k).unwrap());
            assert_eq!(buf_db.get(k).unwrap().unwrap().as_slice(), v);
            if i < BUFFER_SIZE {
                assert!(mem_db.has(k).unwrap());
                assert_eq!(mem_db.get(k).unwrap().unwrap().as_slice(), v);
            } else {
                assert!(!mem_db.has(k).unwrap());
            }
        }

        drop(buf_db);

        for (k, v) in records.iter() {
            assert!(mem_db.has(k).unwrap());
            assert_eq!(mem_db.get(k).unwrap().unwrap().as_slice(), v);
        }
    }
}
