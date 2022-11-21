// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use cid::{multihash, Cid};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;
use fvm_ipld_encoding::CborStore;

/// System actor state.
#[derive(Default, Deserialize_tuple, Serialize_tuple)]
pub struct State {
    // builtin actor registry: Vec<(String, Cid)>
    pub builtin_actors: Cid,
}
impl Cbor for State {}

impl State {
    pub fn new<BS: Blockstore>(store: &BS) -> anyhow::Result<Self> {
        let c = store
            .put_cbor(&Vec::<(String, Cid)>::new(), multihash::Code::Blake2b256)
            .map_err(|e| anyhow!("failed to put system state to store: {}", e))?;
        Ok(Self { builtin_actors: c })
    }

    pub fn get_builtin_actors<B: Blockstore>(
        &self,
        store: &B,
    ) -> Result<Vec<(String, Cid)>, String> {
        match store.get_cbor(&self.builtin_actors) {
            Ok(Some(obj)) => Ok(obj),
            Ok(None) => Err("failed to load builtin actor registry; not found".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}
