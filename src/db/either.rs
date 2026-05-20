// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use spire_enum::prelude::delegated_enum;

#[delegated_enum]
pub enum Either<A: Blockstore, B: Blockstore> {
    Left(A),
    Right(B),
}

impl<A: Blockstore, B: Blockstore> Blockstore for Either<A, B> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        delegate_either!(self.get(k))
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        delegate_either!(self.put_keyed(k, block))
    }

    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        delegate_either!(self.has(k))
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
        delegate_either!(self.put(mh_code, block))
    }

    #[allow(clippy::disallowed_types)]
    fn put_many<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (multihash_codetable::Code, fvm_ipld_blockstore::Block<D>)>,
    {
        delegate_either!(self.put_many(blocks))
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        delegate_either!(self.put_many_keyed(blocks))
    }
}
