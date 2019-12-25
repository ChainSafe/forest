use rocksdb::{DB, Options, WriteBatch}
use std::path::Path;

struct RocksDb {
    db: DB
}

// General TODOs methods should apply a where to ensure the values are CBOR compatible

impl DatabaseService for RocksDb {
    pub fn open(path: &Path) -> Result<(), io:Error> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        let db = DB::open(&db_opts, path)?;
        Ok(Self {
            db: db
        })
    }
}

impl Write for RocksDb {
    pub fn write(&self, key: vec<u8>, value: vec<u8>) -> Result<(), io:Error> {
        self.db.put(key, value)?
    }

    pub fn delete(&self, key: vec<u8>) -> Result<(), io:Error> {
        self.db.delete(key)?
    }

    pub fn bulk_write(&self, keys: [vec<u8>], values: [vec<u8>]) -> Result<(), io::Error> {
        let mut batch = WriteBatch::default();
        for (k,v) in keys.iter().zip(values.iter()) {
            batch.put(k, v);
        }
        self.db.write(batch)?
    }

    pub fn bulk_delete(&self, keys: [vec<u8>]) -> Result<(), io::Error> {}
}

impl Read for RocksDb {
    pub fn read(&self, key: vec<u8>) -> Result<vec<u8>, io::Error> {
        self.db
    }

    pub fn exists(&self, key: vec<u8>) -> bool {
        // pinned key 
        let result = self.db.get_pinned(key);
        match result {
            Some(x) => return true,
            None => return false,
        }
    }

    pub fn bulk_read(&self, keys: [vec<u8>]) -> Result<[vec<u8>], io::Error>;
}

