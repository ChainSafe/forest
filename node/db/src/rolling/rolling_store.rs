// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use ahash::{HashMap, HashMapExt};
use chrono::{DateTime, Utc};
use cid::Cid;
use forest_libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use parking_lot::RwLock;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone)]
pub struct TrackingStore<T> {
    pub index: usize,
    pub store: T,
    pub last_read: Arc<RwLock<DateTime<Utc>>>,
    pub last_write: Arc<RwLock<DateTime<Utc>>>,
}

impl<T> TrackingStore<T> {
    pub fn new(index: usize, store: T) -> Self {
        let now = Utc::now();
        Self {
            index,
            store,
            last_read: Arc::new(RwLock::new(now)),
            last_write: Arc::new(RwLock::new(now)),
        }
    }

    pub(crate) fn track_read(&self) {
        let now = Utc::now();
        *self.last_read.write() = now;
        metrics::ROLLING_DB_LAST_READ
            .with_label_values(&[&self.index.to_string()])
            .set(now.timestamp())
    }

    pub(crate) fn track_write(&self) {
        let now = Utc::now();
        *self.last_write.write() = now;
        metrics::ROLLING_DB_LAST_WRITE
            .with_label_values(&[&self.index.to_string()])
            .set(now.timestamp())
    }
}

impl<T> BitswapStoreRead for TrackingStore<T>
where
    T: ReadStore + BitswapStoreRead,
{
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        self.store.contains(cid)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.store.get(cid)
    }
}

impl<T> BitswapStoreReadWrite for TrackingStore<T>
where
    T: ReadWriteStore + BitswapStoreReadWrite,
{
    type Params = <T as BitswapStoreReadWrite>::Params;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.store.insert(block)
    }
}

#[derive(Debug)]
pub struct RollingStore<T> {
    // capacity: usize,
    root_dir: PathBuf,
    cache: Arc<RwLock<HashMap<usize, TrackingStore<T>>>>,
    // TODO: lookup in order
    // order: Arc<RwLock<BinaryHeap<usize>>>,
}

impl<T> RollingStore<T>
where
    T: ReadWriteStore + IndexedStore + Clone + 'static,
{
    pub fn new(
        // capacity: usize,
        root_dir: PathBuf,
    ) -> Self {
        let cache = Arc::new(RwLock::new(HashMap::new()));
        if let Ok(dir) = std::fs::read_dir(&root_dir) {
            let mut index: Vec<usize> = dir
                .flatten()
                .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or_default())
                .map(|entry| {
                    entry
                        .file_name()
                        .as_os_str()
                        .to_str()
                        .unwrap_or_default()
                        .parse::<i64>()
                        .unwrap_or(-1)
                })
                .filter(|index| index >= &0)
                .map(|i| i as usize)
                .collect();

            if !index.is_empty() {
                index.sort_by(|a, b| b.cmp(a));
                let mut cache = cache.write();
                index
                    .into_iter()
                    // .take(capacity)
                    .for_each(|i| {
                        if let Ok(store) = T::open(root_dir.clone(), i) {
                            cache.insert(i, TrackingStore::new(i, store));
                        }
                    });
            }
        }

        Self {
            // capacity,
            root_dir,
            cache,
        }
    }

    pub fn get_writable_store(&self, index: usize) -> anyhow::Result<TrackingStore<T>> {
        let store_opt = {
            let cache = self.cache.read();
            cache.get(&index).cloned()
        };
        if let Some(store) = store_opt {
            // log::info!("get_writable_store {index} cache hit");
            Ok(store)
        } else {
            let mut cache = self.cache.write();
            if let Some(store) = cache.get(&index).cloned() {
                // log::info!("get_writable_store {index} cache hit");
                Ok(store)
            } else {
                let store = TrackingStore::new(index, T::open(self.root_dir.clone(), index)?);

                // while cache.len() > self.capacity - 1 {
                //     // TODO: Optimize logic here with `BinaryHeap`
                //     if let Some(min_index) = cache.keys().min().cloned() {
                //         if let Some(db) = cache.remove(&min_index) {
                //             if let Err(err) = db.store.flush() {
                //                 log::warn!("{err}");
                //             }
                //         }
                //     } else {
                //         break;
                //     }
                // }

                cache.insert(index, store.clone());

                log::info!(
                    "rolling store {index} opened from {}",
                    self.root_dir.display()
                );

                Ok(store)
            }
        }
    }

    pub fn access_stats(&self) -> HashMap<usize, (DateTime<Utc>, DateTime<Utc>)> {
        let mut map = HashMap::new();
        for (&k, v) in self.cache.read().iter() {
            map.insert(k, (*v.last_read.read(), *v.last_write.read()));
        }
        map
    }

    pub fn delete_on<F>(&self, f: F)
    where
        F: Fn(usize) -> bool,
    {
        let mut cache = self.cache.write();
        let keys: Vec<_> = cache.keys().cloned().collect();
        for k in keys {
            if f(k) {
                if let Some(db) = cache.remove(&k) {
                    if let Err(err) = db.store.delete_db() {
                        log::warn!("Err deleting db {k}: {err}");
                    } else {
                        log::warn!(" db {k} deleted");
                    }
                }
            }
        }
    }
}

impl<T> Default for RollingStore<T>
where
    T: ReadWriteStore + IndexedStore + Clone + 'static,
{
    fn default() -> Self {
        let mut dir = std::env::temp_dir();
        dir.push(".forest");
        Self::new(dir)
    }
}

impl<T> ReadStore for RollingStore<T>
where
    T: ReadWriteStore + IndexedStore + 'static,
{
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        for (_, store) in self.cache.read().iter() {
            if let Some(data) = store.read(key.as_ref())? {
                return Ok(Some(data));
            }
        }
        Ok(None)
    }

    fn exists<K>(&self, key: K) -> Result<bool, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        for (_, store) in self.cache.read().iter() {
            if store.exists(key.as_ref())? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // TODO: Merge results, use fallback implementation for now
    // fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, crate::Error>
    // where
    //     K: AsRef<[u8]>,
    // {
    //     todo!()
    // }
}

impl<T> ReadStore for TrackingStore<T>
where
    T: ReadWriteStore + 'static,
{
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        let opt = self.store.read(key)?;
        if opt.is_some() {
            self.track_read();
        }
        Ok(opt)
    }

    fn exists<K>(&self, key: K) -> Result<bool, crate::Error>
    where
        K: AsRef<[u8]>,
    {
        let exists = self.store.exists(key)?;
        if exists {
            self.track_read();
        }
        Ok(exists)
    }

    // TODO: Merge results, use fallback implementation for now
    // fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, crate::Error>
    // where
    //     K: AsRef<[u8]>,
    // {
    //     todo!()
    // }
}

impl<T> ReadWriteStore for TrackingStore<T>
where
    T: ReadWriteStore + 'static,
{
    fn write<K, V>(&self, key: K, value: V) -> Result<(), crate::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.track_write();
        self.store.write(key, value)
    }

    fn delete<K>(&self, key: K) -> Result<(), crate::Error>
    where
        K: AsRef<[u8]>,
    {
        self.track_write();
        self.store.delete(key)
    }
}
