// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::db::{BlockstoreWriteOpsSubscribable, HeaviestTipsetKeyProvider};
use parking_lot::RwLock;
use std::time::{Duration, Instant};

/// A trait for databases that support garbage collection by resetting specific columns.
#[auto_impl::auto_impl(&, Arc)]
pub trait GarbageCollectableDb {
    fn reset_gc_columns(&self) -> anyhow::Result<()>;
}

/// A wrapper around `ParityDb` that provides a method to reset the columns used for garbage collection.
pub struct GarbageCollectableParityDb {
    options: Options,
    db: RwLock<ParityDb>,
}

impl GarbageCollectableParityDb {
    pub fn new(options: Options) -> anyhow::Result<Self> {
        let db = RwLock::new(ParityDb::open_with_options(&options)?);
        Ok(Self { options, db })
    }

    pub fn reset_gc_columns(&self) -> anyhow::Result<()> {
        let mut guard = self.db.write();
        // Close the database before resetting the columns, otherwise parity-db will fail to reset them.
        let tmp_db_dir = tempfile::tempdir()?;
        let tmp = ParityDb::open(tmp_db_dir.path(), &ParityDbConfig::default())?;
        // Close the database by dropping it, and replace it with a temporary one to avoid holding the file handles of the original database.
        drop(std::mem::replace(&mut *guard, tmp));
        let result = self.reset_gc_columns_inner();
        // Reopen the database no matter whether resetting columns succeeds or not
        *guard = ParityDb::open_with_options(&self.options)
            .with_context(|| {
                format!(
                    "failed to reopen parity-db at {}",
                    self.options.path.display()
                )
            })
            .expect("infallible");
        result
    }

    fn reset_gc_columns_inner(&self) -> anyhow::Result<()> {
        const GC_COLUMNS: [u8; 2] = [
            DbColumn::GraphDagCborBlake2b256 as u8,
            DbColumn::GraphFull as u8,
        ];

        let mut options = self.options.clone();
        for col in GC_COLUMNS {
            let start = Instant::now();
            tracing::info!("pruning parity-db column {col}...");
            // Retry for 10 times with 1s interval in case parity-db is still holding some file handles to the column.
            const MAX_RETRIES: usize = 10;
            for i in 1..=MAX_RETRIES {
                match parity_db::Db::reset_column(&mut options, col, None) {
                    Ok(_) => break,
                    Err(_) if i < MAX_RETRIES => {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                    Err(e) => anyhow::bail!(
                        "failed to reset parity-db column {col} after {MAX_RETRIES} attempts: {e}"
                    ),
                }
            }
            tracing::info!(
                "pruned parity-db column {col}, took {}",
                humantime::format_duration(start.elapsed())
            );
        }
        Ok(())
    }
}

impl GarbageCollectableDb for GarbageCollectableParityDb {
    fn reset_gc_columns(&self) -> anyhow::Result<()> {
        self.reset_gc_columns()
    }
}

impl Blockstore for GarbageCollectableParityDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(&*self.db.read(), k)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        Blockstore::put_keyed(&*self.db.read(), k, block)
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        Blockstore::put_many_keyed(&*self.db.read(), blocks)
    }
}

impl HeaviestTipsetKeyProvider for GarbageCollectableParityDb {
    fn heaviest_tipset_key(&self) -> anyhow::Result<Option<TipsetKey>> {
        HeaviestTipsetKeyProvider::heaviest_tipset_key(&*self.db.read())
    }

    fn set_heaviest_tipset_key(&self, tsk: &TipsetKey) -> anyhow::Result<()> {
        HeaviestTipsetKeyProvider::set_heaviest_tipset_key(&*self.db.read(), tsk)
    }
}

impl SettingsStore for GarbageCollectableParityDb {
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        SettingsStore::read_bin(&*self.db.read(), key)
    }

    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        SettingsStore::write_bin(&*self.db.read(), key, value)
    }

    fn exists(&self, key: &str) -> anyhow::Result<bool> {
        SettingsStore::exists(&*self.db.read(), key)
    }

    fn setting_keys(&self) -> anyhow::Result<Vec<String>> {
        SettingsStore::setting_keys(&*self.db.read())
    }
}

impl EthMappingsStore for GarbageCollectableParityDb {
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        EthMappingsStore::read_bin(&*self.db.read(), key)
    }

    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()> {
        EthMappingsStore::write_bin(&*self.db.read(), key, value)
    }

    fn exists(&self, key: &EthHash) -> anyhow::Result<bool> {
        EthMappingsStore::exists(&*self.db.read(), key)
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        EthMappingsStore::get_message_cids(&*self.db.read())
    }

    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()> {
        EthMappingsStore::delete(&*self.db.read(), keys)
    }
}

impl PersistentStore for GarbageCollectableParityDb {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        PersistentStore::put_keyed_persistent(&*self.db.read(), k, block)
    }
}

impl BitswapStoreRead for GarbageCollectableParityDb {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        BitswapStoreRead::contains(&*self.db.read(), cid)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        BitswapStoreRead::get(&*self.db.read(), cid)
    }
}

impl BitswapStoreReadWrite for GarbageCollectableParityDb {
    type Hashes = <ParityDb as BitswapStoreReadWrite>::Hashes;

    fn insert(&self, block: &crate::libp2p_bitswap::Block64<Self::Hashes>) -> anyhow::Result<()> {
        BitswapStoreReadWrite::insert(&*self.db.read(), block)
    }
}

impl DBStatistics for GarbageCollectableParityDb {
    fn get_statistics(&self) -> Option<String> {
        DBStatistics::get_statistics(&*self.db.read())
    }
}

impl BlockstoreWriteOpsSubscribable for GarbageCollectableParityDb {
    fn subscribe_write_ops(&self) -> tokio::sync::broadcast::Receiver<(Cid, Vec<u8>)> {
        BlockstoreWriteOpsSubscribable::subscribe_write_ops(&*self.db.read())
    }

    fn unsubscribe_write_ops(&self) {
        BlockstoreWriteOpsSubscribable::unsubscribe_write_ops(&*self.db.read())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::db::car_stream::CarBlock;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn test_reset_gc_columns(blocks: [CarBlock; 10]) -> anyhow::Result<()> {
        let db_path = tempfile::tempdir()?;
        let options = ParityDb::to_options(db_path.path(), &ParityDbConfig::default());
        let db = GarbageCollectableParityDb::new(options)?;
        // insert blocks
        for b in &blocks {
            db.put_keyed(&b.cid, &b.data)?;
        }
        // check blocks are present
        for b in &blocks {
            assert_eq!(Blockstore::get(&db, &b.cid)?.as_ref(), Some(&b.data));
        }
        // reset gc columns
        db.reset_gc_columns()?;
        // check blocks are gone
        for b in &blocks {
            assert_eq!(Blockstore::get(&db, &b.cid)?, None);
        }
        // insert blocks again
        for b in &blocks {
            db.put_keyed(&b.cid, &b.data)?;
        }
        // check blocks are present
        for b in &blocks {
            assert_eq!(Blockstore::get(&db, &b.cid)?.as_ref(), Some(&b.data));
        }
        Ok(())
    }
}
