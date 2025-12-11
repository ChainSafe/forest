// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

pub enum Either<A: Blockstore, B: Blockstore> {
    Left(A),
    Right(B),
}

impl<A: Blockstore, B: Blockstore> Blockstore for Either<A, B> {
    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        match self {
            Self::Left(v) => v.has(k),
            Self::Right(v) => v.has(k),
        }
    }

    #[allow(clippy::disallowed_types)]
    fn put<D>(
        &self,
        mh_code: multihash_codetable::Code,
        block: &fvm_ipld_blockstore::Block<D>,
    ) -> anyhow::Result<Cid>
    where
        Self: Sized,
        D: AsRef<[u8]>,
    {
        match self {
            Self::Left(v) => v.put(mh_code, block),
            Self::Right(v) => v.put(mh_code, block),
        }
    }

    #[allow(clippy::disallowed_types)]
    fn put_many<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (multihash_codetable::Code, fvm_ipld_blockstore::Block<D>)>,
    {
        match self {
            Self::Left(v) => v.put_many(blocks),
            Self::Right(v) => v.put_many(blocks),
        }
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        match self {
            Self::Left(v) => v.put_many_keyed(blocks),
            Self::Right(v) => v.put_many_keyed(blocks),
        }
    }

    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match self {
            Self::Left(v) => v.get(k),
            Self::Right(v) => v.get(k),
        }
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        match self {
            Self::Left(v) => v.put_keyed(k, block),
            Self::Right(v) => v.put_keyed(k, block),
        }
    }
}
