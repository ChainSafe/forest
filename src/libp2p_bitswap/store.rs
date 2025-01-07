// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use multihash_derive::MultihashDigest;
use std::ops::Deref;
use std::{marker::PhantomData, sync::Arc};

/// Trait implemented by a block store for reading.
pub trait BitswapStoreRead {
    /// A have query needs to know if the block store contains the block.
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool>;

    /// A block query needs to retrieve the block from the store.
    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>>;
}

/// Trait implemented by a block store for reading and writing.
pub trait BitswapStoreReadWrite: BitswapStoreRead + Send + Sync + 'static {
    /// The hashes parameters.
    type Hashes: MultihashDigest<64>;

    /// A block response needs to insert the block into the store.
    fn insert(&self, block: &Block64<Self::Hashes>) -> anyhow::Result<()>;
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
    type Hashes = <T as BitswapStoreReadWrite>::Hashes;

    fn insert(&self, block: &Block64<Self::Hashes>) -> anyhow::Result<()> {
        BitswapStoreReadWrite::insert(self.as_ref(), block)
    }
}

pub type Block64<H> = Block<H, 64>;

/// Block
#[derive(Clone, Debug)]
pub struct Block<H, const S: usize> {
    /// Content identifier.
    cid: Cid,
    /// Binary data.
    data: Vec<u8>,
    _pd: PhantomData<H>,
}

impl<H, const S: usize> Deref for Block<H, S> {
    type Target = Cid;

    fn deref(&self) -> &Self::Target {
        &self.cid
    }
}

impl<H, const S: usize> PartialEq for Block<H, S> {
    fn eq(&self, other: &Self) -> bool {
        self.cid == other.cid
    }
}

impl<H, const S: usize> Eq for Block<H, S> {}

impl<H: MultihashDigest<S>, const S: usize> Block<H, S> {
    /// Creates a new block. Returns an error if the hash doesn't match
    /// the data.
    pub fn new(cid: Cid, data: Vec<u8>) -> anyhow::Result<Self> {
        Self::verify_cid(&cid, &data)?;
        Ok(Self {
            cid,
            data,
            _pd: Default::default(),
        })
    }

    /// Returns the [`Cid`].
    pub fn cid(&self) -> &Cid {
        &self.cid
    }

    /// Returns the payload.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    fn verify_cid(cid: &Cid, payload: &[u8]) -> anyhow::Result<()> {
        let code = cid.hash().code();
        let mh = H::try_from(code)
            .map_err(|_| anyhow::anyhow!("unsupported multihash code {code}"))?
            .digest(payload);
        if mh.digest() != cid.hash().digest() {
            anyhow::bail!("invalid multihash digest");
        }
        Ok(())
    }
}
