// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Error;
use cid::Cid;
use db::{MemoryDB, Read, RocksDb, Write};
use encoding::Cbor;

pub trait BlockStore: Read + Write {
    fn get(&self, cid: Cid) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.read(cid.to_bytes())?)
    }
    fn put<C: Cbor>(&self, obj: &C) -> Result<Cid, Error> {
        let bz = obj.marshal_cbor()?;
        let cid = Cid::from_bytes_default(&bz)?;
        self.write(cid.to_bytes(), bz)?;
        Ok(cid)
    }
}

impl BlockStore for MemoryDB {}
impl BlockStore for RocksDb {}
