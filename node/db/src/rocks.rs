use super::errors::Error;
use super::traits::{Read, Write};
use rocksdb::{Options, WriteBatch, DB};
use std::path::Path;

#[derive(Debug)]
pub struct RocksDb {
    pub db: DB,
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
    fn write(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), Error> {
        self.db.put(key, value)?;
        Ok(())
    }

    fn delete(&self, key: Vec<u8>) -> Result<(), Error> {
        self.db.delete(key)?;
        Ok(())
    }

    fn bulk_write(&self, keys: &[Vec<u8>], values: &[Vec<u8>]) -> Result<(), Error> {
        let mut batch = WriteBatch::default();
        for (k, v) in keys.iter().zip(values.iter()) {
            batch.put(k, v)?;
        }
        self.db.write(batch)?;
        Ok(())
    }

    fn bulk_delete(&self, _keys: &[Vec<u8>]) -> Result<(), Error> {
        Ok(())
    }
}

impl Read for RocksDb {
    fn read(&self, key: Vec<u8>) -> Result<Vec<u8>, Error> {
        match self.db.get(key) {
            Ok(Some(value)) => Ok(value),
            // TODO figure out how to actually handle this
            Ok(None) => Err(Error::NoValue),
            Err(e) => Err(Error::from(e)),
        }
    }

    fn exists(&self, key: Vec<u8>) -> Result<bool, Error> {
        // pinned key
        let result = self.db.get_pinned(key);
        match result {
            Ok(val) => Ok(val.is_some()),
            Err(e) => Err(Error::from(e)),
        }
    }

    // fn bulk_read(&self, keys: &[Vec<u8>]) -> Result<&[Vec<u8>], Error> {
    //     let v = [vec![]];
    //     Ok(&v)
    // }
}
