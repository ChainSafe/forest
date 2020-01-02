use super::errors::Error;
use super::{Read, Write};
use rocksdb::{Options, WriteBatch, DB};
use std::path::Path;

#[derive(Debug)]
pub struct RocksDb {
    db: DB,
}

impl RocksDb {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        let db = DB::open(&db_opts, path)?;
        Ok(Self { db })
    }
}

impl Write for RocksDb {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Ok(self.db.put(key, value)?)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.delete(key)?)
    }

    fn bulk_write<K, V>(&self, keys: &[K], values: &[V]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let mut batch = WriteBatch::default();
        for (k, v) in keys.iter().zip(values.iter()) {
            batch.put(k, v)?;
        }
        Ok(self.db.write(batch)?)
    }

    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        for k in keys.iter() {
            self.db.delete(k)?;
        }
        Ok(())
    }
}

impl Read for RocksDb {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.get(key).map_err(Error::from)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db
            .get_pinned(key)
            .map(|v| v.is_some())
            .map_err(Error::from)
    }

    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        let mut v = Vec::with_capacity(keys.len());
        for k in keys.iter() {
            match self.db.get(k) {
                Ok(val) => v.push(val),
                Err(e) => return Err(Error::from(e)),
            }
        }
        Ok(v)
    }
}
