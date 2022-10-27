// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{Code, MultihashDigest},
    Cid,
};
use forest_encoding::{de::DeserializeOwned, ser::Serialize};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{from_slice, to_vec, DAG_CBOR};

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

    /// Put an object in the block store and return the Cid identifier.
    fn put_obj<S>(&self, obj: &S, code: Code) -> anyhow::Result<Cid>
    where
        S: Serialize,
    {
        let bytes = to_vec(obj)?;
        self.put_raw(bytes, code)
    }

    /// Put raw bytes in the block store and return the Cid identifier.
    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> anyhow::Result<Cid> {
        let cid = Cid::new_v1(DAG_CBOR, code.digest(&bytes));
        self.put_keyed(&cid, &bytes)?;
        Ok(cid)
    }

    /// Batch put CBOR objects into block store and returns vector of CIDs
    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> anyhow::Result<Vec<Cid>>
    where
        Self: Sized,
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        let keyed_objects = values
            .into_iter()
            .map(|value| {
                let bytes = to_vec(value)?;
                let cid = Cid::new_v1(DAG_CBOR, code.digest(&bytes));
                Ok((cid, bytes))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let cids = keyed_objects
            .iter()
            .map(|(cid, _)| cid.to_owned())
            .collect();

        self.put_many_keyed(keyed_objects)?;

        Ok(cids)
    }
}

impl<T: fvm_ipld_blockstore::Blockstore> BlockstoreExt for T {}
