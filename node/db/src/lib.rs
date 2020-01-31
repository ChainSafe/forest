// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
mod memory;
mod rocks;

pub use errors::Error;
pub use memory::*;
pub use rocks::*;

pub trait DatabaseService {
    fn open(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub trait Write {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>;
    fn bulk_write<K, V>(&self, keys: &[K], values: &[V]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>;
}

pub trait Read {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>;
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>;
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>;
}
