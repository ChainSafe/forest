// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chrono::Utc;
use cid::Cid;
use forest_libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use forest_utils::db::file_backed_obj::FileBackedObject;
use fvm_ipld_blockstore::Blockstore;
use human_repr::HumanCount;
use parking_lot::RwLock;

use super::*;
use crate::*;

impl Blockstore for RollingDB {
    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        for db in self.db_queue.read().iter() {
            if let Ok(true) = Blockstore::has(db, k) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        for db in self.db_queue.read().iter() {
            if let Ok(Some(v)) = Blockstore::get(db, k) {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    fn put<D>(
        &self,
        mh_code: cid::multihash::Code,
        block: &fvm_ipld_blockstore::Block<D>,
    ) -> anyhow::Result<Cid>
    where
        Self: Sized,
        D: AsRef<[u8]>,
    {
        Blockstore::put(&self.current(), mh_code, block)
    }

    fn put_many<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (cid::multihash::Code, fvm_ipld_blockstore::Block<D>)>,
    {
        Blockstore::put_many(&self.current(), blocks)
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        Blockstore::put_many_keyed(&self.current(), blocks)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        Blockstore::put_keyed(&self.current(), k, block)
    }
}

impl Store for RollingDB {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        for db in self.db_queue.read().iter() {
            if let Ok(Some(v)) = Store::read(db, key.as_ref()) {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    fn exists<K>(&self, key: K) -> Result<bool, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        for db in self.db_queue.read().iter() {
            if let Ok(true) = Store::exists(db, key.as_ref()) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), crate::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Store::write(&self.current(), key, value)
    }

    fn delete<K>(&self, key: K) -> Result<(), crate::Error>
    where
        K: AsRef<[u8]>,
    {
        Store::delete(&self.current(), key)
    }

    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), crate::Error> {
        Store::bulk_write(&self.current(), values)
    }

    fn flush(&self) -> Result<(), crate::Error> {
        Store::flush(&self.current())
    }
}

impl BitswapStoreRead for RollingDB {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        for db in self.db_queue.read().iter() {
            if let Ok(true) = BitswapStoreRead::contains(db, cid) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        for db in self.db_queue.read().iter() {
            if let Ok(Some(v)) = BitswapStoreRead::get(db, cid) {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }
}

impl BitswapStoreReadWrite for RollingDB {
    type Params = <Db as BitswapStoreReadWrite>::Params;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        BitswapStoreReadWrite::insert(&self.current(), block)
    }
}

impl DBStatistics for RollingDB {
    fn get_statistics(&self) -> Option<String> {
        DBStatistics::get_statistics(&self.current())
    }
}

impl FileBackedObject for DbIndex {
    fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_yaml::to_string(self)?.as_bytes().to_vec())
    }

    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_yaml::from_slice(bytes)?)
    }
}

impl Drop for RollingDB {
    fn drop(&mut self) {
        if let Err(err) = self.flush() {
            warn!(
                "Error flushing rolling db under {}: {err}",
                self.db_root.display()
            );
        }
    }
}

impl RollingDB {
    pub fn load_or_create(db_root: PathBuf, db_config: DbConfig) -> anyhow::Result<Self> {
        if !db_root.exists() {
            std::fs::create_dir_all(db_root.as_path())?;
        }
        let (db_index, db_queue) = load_db_queue(db_root.as_path(), &db_config)?;
        let rolling = Self {
            db_root: db_root.into(),
            db_config: db_config.into(),
            db_index: RwLock::new(db_index).into(),
            db_queue: RwLock::new(db_queue).into(),
        };

        if rolling.db_queue.read().is_empty() {
            let (name, db) = rolling.create_untracked()?;
            rolling.track_as_current(name, db)?;
        }

        Ok(rolling)
    }

    pub fn track_as_current(&self, name: String, db: Db) -> anyhow::Result<()> {
        self.db_queue.write().push_front(db);
        let mut db_index = self.db_index.write();
        db_index.inner_mut().db_names.push_front(name);
        db_index.flush_to_file()
    }

    pub fn create_untracked(&self) -> anyhow::Result<(String, Db)> {
        let name = Utc::now().timestamp_millis().to_string();
        let db = open_db(&self.db_root.join(&name), &self.db_config)?;
        Ok((name, db))
    }

    pub fn clean_tracked(&self, n_db_to_reserve: usize, delete: bool) -> anyhow::Result<()> {
        anyhow::ensure!(n_db_to_reserve > 0);

        let mut db_index = self.db_index.write();
        let mut db_queue = self.db_queue.write();
        while db_queue.len() > n_db_to_reserve {
            if let Some(db) = db_queue.pop_back() {
                db.flush()?;
            }
            if let Some(name) = db_index.inner_mut().db_names.pop_back() {
                info!("Closing DB {name}");
                if delete {
                    let db_path = self.db_root.join(name);
                    delete_db(&db_path);
                }
            }
        }

        db_index.flush_to_file()
    }

    pub fn clean_untracked(&self) -> anyhow::Result<()> {
        if let Ok(dir) = std::fs::read_dir(self.db_root.as_path()) {
            let db_index = self.db_index.read();
            dir.flatten()
                .filter(|entry| {
                    entry.path().is_dir()
                        && db_index
                            .inner()
                            .db_names
                            .iter()
                            .all(|name| entry.path() != self.db_root.join(name).as_path())
                })
                .for_each(|entry| delete_db(&entry.path()));
        }
        Ok(())
    }

    pub fn size_in_bytes(&self) -> anyhow::Result<u64> {
        Ok(fs_extra::dir::get_size(self.db_root.as_path())?)
    }

    pub fn size(&self) -> usize {
        self.db_queue.read().len()
    }

    fn current(&self) -> Db {
        self.db_queue
            .read()
            .get(0)
            .cloned()
            .expect("RollingDB should contain at least one DB reference")
    }
}

fn load_db_queue(
    db_root: &Path,
    db_config: &DbConfig,
) -> anyhow::Result<(FileBacked<DbIndex>, VecDeque<Db>)> {
    let mut db_index =
        FileBacked::load_from_file_or_create(db_root.join("db_index.yaml"), Default::default)?;
    let mut db_queue = VecDeque::new();
    let index_inner_mut: &mut DbIndex = db_index.inner_mut();
    for i in (0..index_inner_mut.db_names.len()).rev() {
        let name = index_inner_mut.db_names[i].as_str();
        let db_path = db_root.join(name);
        if !db_path.is_dir() {
            index_inner_mut.db_names.remove(i);
            continue;
        }
        match open_db(&db_path, db_config) {
            Ok(db) => db_queue.push_front(db),
            Err(err) => {
                index_inner_mut.db_names.remove(i);
                warn!("Failed to open database under {}: {err}", db_path.display());
            }
        }
    }

    db_index.flush_to_file()?;
    Ok((db_index, db_queue))
}

fn delete_db(db_path: &Path) {
    let size = fs_extra::dir::get_size(db_path).unwrap_or_default();
    if let Err(err) = std::fs::remove_dir_all(db_path) {
        warn!(
            "Error deleting database under {}, size: {}. {err}",
            db_path.display(),
            size.human_count_bytes()
        );
    } else {
        info!(
            "Deleted database under {}, size: {}",
            db_path.display(),
            size.human_count_bytes()
        );
    }
}
