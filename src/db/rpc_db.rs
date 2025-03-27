// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use ahash::HashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::RwLock;

use crate::rpc::{prelude::ChainReadObj, Client, RpcMethodExt as _};

/// A blocktore backed by Filecoin RPC APIs
pub struct RpcDb {
    client: Arc<Client>,
    cache: RwLock<HashMap<Cid, Option<Vec<u8>>>>,
}

impl RpcDb {
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client,
            cache: Default::default(),
        }
    }
}

impl Blockstore for RpcDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(v) = self.cache.read().get(k) {
            return Ok(v.clone());
        }
        let bytes = ChainReadObj::call_sync(self.client.clone(), (k.clone(),)).ok();
        self.cache.write().insert(*k, bytes.clone());
        Ok(bytes)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.cache.write().insert(*k, Some(block.to_vec()));
        Ok(())
    }
}
