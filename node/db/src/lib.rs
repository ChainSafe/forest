// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
mod memory;

#[cfg(feature = "rocksdb")]
pub mod rocks;

#[cfg(feature = "sled")]
pub mod sled;

pub use errors::Error;
pub use memory::MemoryDB;

/// Store interface used as a KV store implementation
pub trait Store {
    /// Read single value from data store and return `None` if key doesn't exist.
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>;

    /// Write a single value to the data store.
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;

    /// Delete value at key.
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>;

    /// Returns `Ok(true)` if key exists in store
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>;

    /// Read slice of keys and return a vector of optional values.
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().map(|key| self.read(key)).collect()
    }

    /// Write slice of KV pairs.
    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        values
            .iter()
            .try_for_each(|(key, value)| self.write(key, value))
    }

    /// Bulk delete keys from the data store.
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().try_for_each(|key| self.delete(key))
    }
}
