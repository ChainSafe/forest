// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::Store;
use crossbeam::atomic::AtomicCell;
pub use sled::{Batch, Config, Db, Mode};
use std::convert::TryInto;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::vec;

/// LumberjackDB instance this satisfies the [Store] interface.
/// Uses Sled as an index into an append-only log file.
pub struct LumberjackDb {
    pub index: Db,
    log: Mutex<File>,
    stats: Stats,
}

/// DB stats
/// key_bytes: Bytes written to disk as keys,
/// val_count: Could differ from number of keys stored, since log is append-only
/// val_bytes: val_count * 16 byte loc tuple
/// val_bytes_saved:  // val.len() - 16
/// TODO: Replace with Prometheus metrics when those are available
struct Stats {
    key_bytes: AtomicCell<u64>,
    val_count: AtomicCell<u64>,
    val_bytes: AtomicCell<u64>,
    val_bytes_saved: AtomicCell<i64>,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            key_bytes: Default::default(),
            val_count: Default::default(),
            val_bytes: Default::default(),
            val_bytes_saved: Default::default(),
        }
    }
}

fn open_log(path: PathBuf) -> Result<File, Error> {
    OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
        .map_err(Error::from)
}

/// LumberjackDB is a new alternative blockstore implementation instead of RocksDB.
/// It is designed so values are written to a separate file in order to reduce index size.
///
/// Usage:
/// ```no_run
/// use forest_db::lumberjack::LumberjackDb;
///
/// let mut db = LumberjackDb::open("test_db");
/// ```
impl LumberjackDb {
    pub fn open<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let mut fs_path = PathBuf::new();
        fs_path.push(path);
        fs_path.push("index");

        let config = Config::default()
            .path(&fs_path)
            .mode(sled::Mode::HighThroughput)
            // 4 gb
            .cache_capacity(1024 * 1024 * 1024 * 4);

        fs_path.pop();
        fs_path.push("log");

        Ok(Self {
            index: config.open()?,
            log: Mutex::new(open_log(fs_path)?),
            stats: Default::default(),
        })
    }

    /// Open a db with custom configuration.
    pub fn open_with_config(config: Config) -> Result<Self, Error> {
        let mut fs_path = PathBuf::new();
        fs_path.push(config.path.to_owned());
        fs_path.pop();
        fs_path.push("log");

        Ok(Self {
            index: config.open()?,
            log: Mutex::new(open_log(fs_path)?),
            stats: Default::default(),
        })
    }

    /// Initialize a sled in memory database. This will not persist data.
    pub fn temporary() -> Result<Self, Error> {
        let config = sled::Config::default().temporary(true);

        let mut fs_path = PathBuf::new();
        fs_path.push(config.path.to_owned());
        fs_path.pop();
        fs_path.push("log");

        Ok(Self {
            index: config.open()?,
            log: Mutex::new(open_log(fs_path)?),
            stats: Default::default(),
        })
    }
}

impl Store for LumberjackDb {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let key_len = mem::size_of_val(&key);
        let val_len = mem::size_of_val(&value);
        let loc_len = 16;

        self.stats.key_bytes.fetch_add(key_len as u64);
        self.stats.val_count.fetch_add(1);
        self.stats.val_bytes.fetch_add(loc_len);
        self.stats
            .val_bytes_saved
            .fetch_add(val_len as i64 - loc_len as i64);

        self.log.lock().unwrap().write_all(value.as_ref())?;
        self.log.lock().unwrap().flush()?;

        let offset = self.log.lock().unwrap().stream_position()?.to_be_bytes();
        let limit = val_len.to_be_bytes();

        let val = [offset, limit].concat();

        self.index.insert(key, val)?;

        Ok(())
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.index.remove(key)?;
        Ok(())
    }

    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let mut batch = Batch::default();
        for (k, v) in values {
            batch.insert(k.as_ref(), v.as_ref());
        }
        // TODO: write to log
        Ok(self.index.apply_batch(batch)?)
    }

    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        match self.index.get(key)? {
            Some(loc) => {
                let offset = u64::from_be_bytes(loc.subslice(0, 8).as_ref().try_into()?);
                let limit = u64::from_be_bytes(loc.subslice(8, 8).as_ref().try_into()?);
                let mut buf = vec![0u8; limit as usize];

                self.log.lock().unwrap().seek(SeekFrom::Start(offset))?;
                self.log.lock().unwrap().read_exact(&mut buf)?;

                Ok(Some(buf))
            }
            None => Ok(None),
        }
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.index.contains_key(key)?)
    }
}
