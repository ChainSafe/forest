use std::path::Path;

use sled::Db;

use crate::error::Result;
use crate::kv_store::KeyValueStore;

pub struct SledKvs {
    db: Db,
}

impl KeyValueStore for SledKvs {
    fn initialize<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Db::start_default(path)?;
        Ok(SledKvs { db })
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.set(key, value)?;
        let _ = self.db.flush()?;
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let value = self.db.get(key)?;
        Ok(value.map(|x| x.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha() {
        let metadata_dir = tempfile::tempdir().unwrap();

        let db = SledKvs::initialize(metadata_dir).unwrap();

        let k_a = b"key-xx";
        let k_b = b"key-yy";
        let v_a = b"value-aa";
        let v_b = b"value-bb";

        db.put(k_a, v_a).unwrap();
        db.put(k_b, v_b).unwrap();

        let opt = db.get(k_a).unwrap();
        assert_eq!(format!("{:x?}", opt.unwrap()), format!("{:x?}", v_a));
    }
}
