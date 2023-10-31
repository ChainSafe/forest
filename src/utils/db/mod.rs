// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[deprecated]
pub mod car_index;
pub mod car_stream;
pub mod car_util;

use cid::{
    multihash::{Code, MultihashDigest},
    Cid,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_ipld_encoding::{to_vec, DAG_CBOR};

use serde::ser::Serialize;

/// Extension methods for inserting and retrieving IPLD data with CIDs
pub trait BlockstoreExt: Blockstore {
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

/// Extension methods for [`CborStore`] that omits default multihash code from its APIs
pub trait CborStoreExt: CborStore {
    /// Default multihash code is [`cid::multihash::Code::Blake2b256`]
    /// See <https://github.com/ipfs/go-ipld-cbor/blob/v0.0.6/store.go#L92>
    /// ```go
    /// mhType := uint64(mh.BLAKE2B_MIN + 31)
    /// // 45569 + 31 = 45600 = 0xb220
    /// ```
    fn default_code() -> cid::multihash::Code {
        cid::multihash::Code::Blake2b256
    }

    /// A wrapper of [`CborStore::put_cbor`] that omits code parameter to match store API in go
    fn put_cbor_default<S: serde::ser::Serialize>(&self, obj: &S) -> anyhow::Result<Cid> {
        self.put_cbor(obj, Self::default_code())
    }
}

impl<T: CborStore> CborStoreExt for T {}
