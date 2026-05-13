// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    car::ManyCar,
    parity_db::{GarbageCollectableParityDb, ParityDb},
    *,
};
use crate::{
    libp2p_bitswap::*, tool::subcommands::api_cmd::generate_test_snapshot::ReadOpsTrackingStore,
    utils::ShallowClone,
};
use ambassador::Delegate;
use spire_enum::prelude::delegated_enum;
use std::sync::Arc;

#[derive(Delegate)]
#[delegate(SettingsStore)]
#[delegate(EthMappingsStore)]
#[delegate(BitswapStoreRead)]
#[delegate(BitswapStoreReadWrite)]
#[delegated_enum(impl_conversions)]
pub enum DbImpl {
    ManyCarWithGarbageCollectableParityDb(Arc<ManyCar<Arc<GarbageCollectableParityDb>>>),
    ManyCarWithMemoryDB(Arc<ManyCar<MemoryDB>>),
    ManyCarParityDb(Arc<ManyCar<ParityDb>>),
    Memory(Arc<MemoryDB>),
    ReadOpsTrackingManyCarParityDb(Arc<ReadOpsTrackingStore<ManyCar<ParityDb>>>),
    #[cfg(test)]
    Chain4U(Arc<crate::blocks::Chain4U<ManyCar>>),
}

impl ShallowClone for DbImpl {
    fn shallow_clone(&self) -> Self {
        delegate_db_impl!(self.shallow_clone().into())
    }
}

impl Blockstore for DbImpl {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        delegate_db_impl!(self => |i| Blockstore::get(i, k))
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        delegate_db_impl!(self => |i| Blockstore::put_keyed(i, k, block))
    }

    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        delegate_db_impl!(self => |i| Blockstore::has(i, k))
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
        delegate_db_impl!(self => |i| Blockstore::put(i, mh_code, block))
    }

    #[allow(clippy::disallowed_types)]
    fn put_many<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (multihash_codetable::Code, fvm_ipld_blockstore::Block<D>)>,
    {
        delegate_db_impl!(self => |i| Blockstore::put_many(i, blocks))
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        delegate_db_impl!(self => |i| Blockstore::put_many_keyed(i, blocks))
    }
}
