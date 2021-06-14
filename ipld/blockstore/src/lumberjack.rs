// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockStore;
use cid::{Cid, Code};
use db::{
    lumberjack::{Batch, LumberjackDb},
    Store,
};
use encoding::{ser::Serialize, to_vec};
use std::error::Error as StdError;

impl BlockStore for LumberjackDb {
    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let cid = cid::new_from_cbor(&bytes, code);
        let cid_bytes = cid.to_bytes();
        // Can do a unique compare and swap here, should only need to write when entry doesn't
        // exist as all Cids "should" be unique. If the value exists, ignore.
        match self.read(&cid_bytes)? {
            Some(_) => {}
            None => {
                self.write(&cid_bytes, bytes)?;
            }
        }

        Ok(cid)
    }

    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> Result<Vec<Cid>, Box<dyn StdError>>
    where
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        let mut batch = Batch::default();
        let cids: Vec<Cid> = values
            .into_iter()
            .map(|v| {
                let bz = to_vec(v)?;
                let cid = cid::new_from_cbor(&bz, code);
                batch.insert(cid.to_bytes(), bz);
                Ok(cid)
            })
            .collect::<Result<_, Box<dyn StdError>>>()?;
        // TODO: insert into log
        self.index.apply_batch(batch)?;

        Ok(cids)
    }
}
