// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::from_slice;
use serde::de::DeserializeOwned;

pub fn get_obj<T>(store: &impl Blockstore, cid: &Cid) -> anyhow::Result<Option<T>>
where
    T: DeserializeOwned,
{
    match store.get(cid)? {
        Some(bz) => Ok(Some(from_slice(&bz)?)),
        None => Ok(None),
    }
}
