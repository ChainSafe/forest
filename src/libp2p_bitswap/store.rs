// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use ambassador::delegatable_trait;
use multihash_derive::MultihashDigest;
use std::marker::PhantomData;
use std::ops::Deref;

/// Trait implemented by a block store for reading.
#[auto_impl::auto_impl(&, Arc)]
#[delegatable_trait]
pub trait BitswapStoreRead {
    /// A have query needs to know if the block store contains the block.
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool>;

    /// A block query needs to retrieve the block from the store.
    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>>;
}

/// Trait implemented by a block store for reading and writing.
#[auto_impl::auto_impl(&, Arc)]
#[delegatable_trait]
pub trait BitswapStoreReadWrite: BitswapStoreRead + Send + Sync + 'static {
    /// The hashes parameters.
    type Hashes: MultihashDigest<64>;

    /// A block response needs to insert the block into the store.
    fn insert(&self, block: &Block64<Self::Hashes>) -> anyhow::Result<()>;
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
