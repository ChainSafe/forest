// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod car_stream;
pub mod car_util;

use crate::utils::multihash::prelude::*;
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_ipld_encoding::{DAG_CBOR, to_vec};
#[allow(clippy::disallowed_types)]
use multihash_codetable::Code;

use serde::ser::Serialize;

/// Extension methods for inserting and retrieving IPLD data with `CIDs`
pub trait BlockstoreExt: Blockstore {
    /// Batch put CBOR objects into block store and returns vector of `CIDs`
    #[allow(clippy::disallowed_types)]
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

    /// Gets the block from the blockstore. Return an error when not found.
    fn get_required(&self, cid: &Cid) -> anyhow::Result<Vec<u8>> {
        self.get(cid)?
            .with_context(|| format!("Entry not found in block store: cid={cid}"))
    }
}

impl<T: fvm_ipld_blockstore::Blockstore> BlockstoreExt for T {}

/// Extension methods for [`CborStore`] that omits default multihash code from its `APIs`
pub trait CborStoreExt: CborStore {
    /// Default multihash code is [`cid::multihash::Code::Blake2b256`]
    /// See <https://github.com/ipfs/go-ipld-cbor/blob/v0.0.6/store.go#L92>
    /// ```go
    /// mhType := uint64(mh.BLAKE2B_MIN + 31)
    /// // 45569 + 31 = 45600 = 0xb220
    /// ```
    #[allow(clippy::disallowed_types)]
    fn default_code() -> Code {
        Code::Blake2b256
    }

    /// A wrapper of [`CborStore::put_cbor`] that omits code parameter to match store API in go
    fn put_cbor_default<S: serde::ser::Serialize>(&self, obj: &S) -> anyhow::Result<Cid> {
        self.put_cbor(obj, Self::default_code())
    }

    /// Get typed object from block store by `CID`. Return an error when not found.
    fn get_cbor_required<T>(&self, cid: &Cid) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.get_cbor(cid)?.with_context(|| {
            format!(
                "Entry not found in cbor store: cid={cid}, type={}",
                std::any::type_name::<T>()
            )
        })
    }
}

impl<T: CborStore> CborStoreExt for T {}
