// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    car::ManyCar,
    parity_db::{GarbageCollectableDb, GarbageCollectableParityDb, ParityDb},
    *,
};
use crate::{
    db::car::ReloadableManyCar, libp2p_bitswap::*, prelude::*,
    tool::subcommands::api_cmd::generate_test_snapshot::ReadOpsTrackingStore,
};
use ambassador::Delegate;
use spire_enum::prelude::delegated_enum;
use std::path::PathBuf;

#[derive(Delegate)]
#[delegate(SettingsStore)]
#[delegate(EthMappingsStore)]
#[delegate(HeaviestTipsetKeyProvider)]
#[delegate(BitswapStoreRead)]
#[delegate(BitswapStoreReadWrite)]
#[delegated_enum(impl_conversions)]
pub enum DbImpl {
    ManyCarWithGarbageCollectableParityDb(Arc<ManyCar<Arc<GarbageCollectableParityDb>>>),
    ManyCarWithMemoryDB(Arc<ManyCar<MemoryDB>>),
    ManyCarParityDb(Arc<ManyCar<ParityDb>>),
    Memory(Arc<MemoryDB>),
    ReadOpsTrackingManyCarParityDb(Arc<ReadOpsTrackingStore<ManyCar<ParityDb>>>),
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

impl GarbageCollectableDb for DbImpl {
    fn reset_gc_columns(&self) -> anyhow::Result<()> {
        match self {
            Self::ManyCarWithGarbageCollectableParityDb(db) => db.reset_gc_columns(),
            _ => anyhow::bail!("db is not garbage collectable"),
        }
    }
}

impl BlockstoreWriteOpsSubscribable for DbImpl {
    fn subscribe_write_ops(
        &self,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<Vec<(Cid, bytes::Bytes)>>> {
        if let Self::ManyCarWithGarbageCollectableParityDb(db) = self {
            db.subscribe_write_ops()
        } else {
            anyhow::bail!("not supported")
        }
    }

    fn unsubscribe_write_ops(&self) {
        if let Self::ManyCarWithGarbageCollectableParityDb(db) = self {
            db.unsubscribe_write_ops();
        }
    }
}

impl ReloadableManyCar for DbImpl {
    fn clear_and_reload_cars(&self, files: impl Iterator<Item = PathBuf>) -> anyhow::Result<()> {
        match self {
            Self::ManyCarWithGarbageCollectableParityDb(db) => db.clear_and_reload_cars(files),
            Self::ManyCarWithMemoryDB(db) => db.clear_and_reload_cars(files),
            Self::ManyCarParityDb(db) => db.clear_and_reload_cars(files),
            _ => anyhow::bail!("not supported"),
        }
    }

    fn heaviest_car_tipset(&self) -> anyhow::Result<Tipset> {
        match self {
            Self::ManyCarWithGarbageCollectableParityDb(db) => db.heaviest_car_tipset(),
            Self::ManyCarWithMemoryDB(db) => db.heaviest_car_tipset(),
            Self::ManyCarParityDb(db) => db.heaviest_car_tipset(),
            _ => anyhow::bail!("not supported"),
        }
    }
}
