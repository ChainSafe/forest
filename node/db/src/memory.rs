use super::Error;
use super::{DatabaseService, Read, Write};
use parking_lot::RwLock;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};

/// A thread-safe `HashMap` wrapper.
pub struct MemoryDB {
    db: RwLock<HashMap<u64, Vec<u8>>>,
}

impl MemoryDB {
    fn db_index<K>(key: K) -> u64
    where
        K: AsRef<[u8]>,
    {
        let mut hasher = DefaultHasher::new();
        key.as_ref().hash::<DefaultHasher>(&mut hasher);
        hasher.finish()
    }
}

impl Clone for MemoryDB {
    fn clone(&self) -> Self {
        Self {
            db: RwLock::new(self.db.read().clone()),
        }
    }
}

impl Default for MemoryDB {
    fn default() -> Self {
        Self {
            db: RwLock::new(HashMap::new()),
        }
    }
}

impl DatabaseService for MemoryDB {}

impl Write for MemoryDB {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.db
            .write()
            .insert(Self::db_index(key), value.as_ref().to_vec());
        Ok(())
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.write().remove(&Self::db_index(key));
        Ok(())
    }

    fn bulk_write<K, V>(&self, keys: &[K], values: &[V]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        for (k, v) in keys.iter().zip(values.iter()) {
            self.db
                .write()
                .insert(Self::db_index(k), v.as_ref().to_vec());
        }
        Ok(())
    }

    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        for k in keys.iter() {
            self.db.write().remove(&Self::db_index(k));
        }
        Ok(())
    }
}

impl Read for MemoryDB {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.read().get(&Self::db_index(key)).cloned())
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.read().contains_key(&Self::db_index(key)))
    }

    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        let mut v = Vec::with_capacity(keys.len());
        for k in keys.iter() {
            v.push(self.db.read().get(&Self::db_index(k)).cloned())
        }
        Ok(v)
    }
}
