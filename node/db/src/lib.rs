// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
mod memory;
mod metrics;
mod utils;

#[cfg(feature = "rocksdb")]
pub mod rocks;
#[cfg(feature = "rocksdb")]
pub mod rocks_config;

#[cfg(feature = "paritydb")]
pub mod parity_db;

pub use errors::Error;
pub use memory::MemoryDB;

const DEFAULT_COLUMN: &str = "default";

/// Store interface used as a KV store implementation
pub trait Store {
    /// Read single value from the default column of the data store and return `None` if key doesn't exist.
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.read_column(key, DEFAULT_COLUMN)
    }

    /// Read single value from the specified column of the data store and return `None` if key doesn't exist.
    fn read_column<K>(&self, key: K, column: &str) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>;

    /// Write a single value to the default column of the data store.
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.write_column(key, value, DEFAULT_COLUMN)
    }

    /// Write a single value to the specified column of the data store.
    fn write_column<K, V>(&self, key: K, value: V, column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;

    /// Delete value at key from the default column.
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.delete_column(key, DEFAULT_COLUMN)
    }

    /// Delete value at key from the specified column.
    fn delete_column<K>(&self, key: K, column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>;

    /// Returns `Ok(true)` if key exists in the default column of the store
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.exists_column(key, DEFAULT_COLUMN)
    }

    /// Returns `Ok(true)` if key exists in the specified column of the store
    fn exists_column<K>(&self, key: K, column: &str) -> Result<bool, Error>
    where
        K: AsRef<[u8]>;

    /// Read slice of keys and return a vector of optional values from the default column.
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().map(|key| self.read(key)).collect()
    }

    /// Read slice of keys and return a vector of optional values from the specified column.
    fn bulk_read_column<K>(&self, keys: &[K], column: &str) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter()
            .map(|key| self.read_column(key, column))
            .collect()
    }

    /// Write slice of KV pairs to the default column.
    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        values
            .iter()
            .try_for_each(|(key, value)| self.write(key, value))
    }

    /// Write slice of KV pairs to the specified column.
    fn bulk_write_column<K, V>(&self, values: &[(K, V)], column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        values
            .iter()
            .try_for_each(|(key, value)| self.write_column(key, value, column))
    }

    /// Bulk delete keys from the default column of the data store.
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().try_for_each(|key| self.delete(key))
    }

    /// Bulk delete keys from the specified column of the data store.
    fn bulk_delete_column<K>(&self, keys: &[K], column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter()
            .try_for_each(|key| self.delete_column(key, column))
    }
}

impl<BS: Store> Store for &BS {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).read(key)
    }

    fn read_column<K>(&self, key: K, column: &str) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).read_column(key, column)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        (*self).write(key, value)
    }

    fn write_column<K, V>(&self, key: K, value: V, column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        (*self).write_column(key, value, column)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).delete(key)
    }

    fn delete_column<K>(&self, key: K, column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).delete_column(key, column)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).exists(key)
    }

    fn exists_column<K>(&self, key: K, column: &str) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).exists_column(key, column)
    }

    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).bulk_read(keys)
    }

    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        (*self).bulk_write(values)
    }

    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).bulk_delete(keys)
    }
}
