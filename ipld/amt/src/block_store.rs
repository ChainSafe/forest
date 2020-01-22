// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Error;
use cid::Cid;
use db::{MemoryDB, Read, RocksDb, Write};

pub trait BlockStore: Read + Write {
    fn get(&self, cid: Cid) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.read(cid.to_bytes())?)
    }
    fn put(&self, bz: &[u8]) -> Result<Cid, Error> {
        let cid = Cid::from_bytes_default(bz)?;
        self.write(cid.to_bytes(), bz)?;
        Ok(cid)
    }
}

impl BlockStore for MemoryDB {}
impl BlockStore for RocksDb {}
