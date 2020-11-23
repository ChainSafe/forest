// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockStore;
use cid::{Cid, Code};
use db::sled::SledDb;
use std::error::Error as StdError;

impl BlockStore for SledDb {
    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let cid = cid::new_from_cbor(&bytes, code);
        // Can do a unique compare and swap here, should only need to write when entry doesn't
        // exist as all Cids "should" be unique. If the value exists, ignore.
        let _ = self
            .db
            .compare_and_swap(cid.to_bytes(), None as Option<&[u8]>, Some(bytes))?;
        Ok(cid)
    }
}
