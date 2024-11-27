// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use std::sync::Arc;

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

impl<T: BitswapStoreRead> BitswapStoreRead for Arc<T> {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        BitswapStoreRead::contains(self.as_ref(), cid)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        BitswapStoreRead::get(self.as_ref(), cid)
    }
}

impl<T: BitswapStoreReadWrite> BitswapStoreReadWrite for Arc<T> {
    /// `fvm_ipld_encoding::DAG_CBOR(0x71)` is covered by
    /// [`libipld::DefaultParams`] under feature `dag-cbor`
    type Params = <T as BitswapStoreReadWrite>::Params;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        BitswapStoreReadWrite::insert(self.as_ref(), block)
    }
}

/// Block
#[derive(Clone)]
pub struct Block<S> {
    _marker: PhantomData<S>,
    /// Content identifier.
    cid: Cid,
    /// Binary data.
    data: Vec<u8>,
}

// /// The store parameters.
// pub trait BitswapStoreParams: std::fmt::Debug + Clone + Send + Sync + Unpin + 'static {
//     /// The multihash type of the store.
//     type Hashes: MultihashDigest<64>;
//     /// The codec type of the store.
//     type Codecs: Codec;
//     /// The maximum block size supported by the store.
//     const MAX_BLOCK_SIZE: usize;
// }

// /// Default store parameters.
// #[derive(Clone, Debug, Default)]
// pub struct DefaultBitswapStoreParams;

// impl BitswapStoreParams for DefaultBitswapStoreParams {
//     const MAX_BLOCK_SIZE: usize = 1_048_576;
//     type Codecs = libipld::IpldCodec;
//     type Hashes = multihash_codetable::Code;
// }
