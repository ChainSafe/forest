// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod builtin;

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::from_slice;
use serde::de::DeserializeOwned;

pub use self::builtin::*;

/// Extension methods for inserting and retrieving IPLD data with CIDs
pub trait BlockstoreExt: Blockstore {
    /// Get typed object from block store by CID
    fn get_obj<T>(&self, cid: &Cid) -> anyhow::Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        match self.get(cid)? {
            Some(bz) => Ok(Some(from_slice(&bz)?)),
            None => Ok(None),
        }
    }
}

impl<T: fvm_ipld_blockstore::Blockstore> BlockstoreExt for T {}
