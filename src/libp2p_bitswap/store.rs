// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libipld::Block;

use crate::libp2p_bitswap::*;

/// Trait implemented by a block store for reading.
pub trait BitswapStoreRead {
    /// A have query needs to know if the block store contains the block.
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool>;

    /// A block query needs to retrieve the block from the store.
    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>>;
}

/// Trait implemented by a block store for reading and writing.
pub trait BitswapStoreReadWrite: BitswapStoreRead + Send + Sync + 'static {
    /// The store parameters.
    type Params: StoreParams;

    /// A block response needs to insert the block into the store.
    fn insert(&self, block: &Block<Self::Params>) -> anyhow::Result<()>;
}
