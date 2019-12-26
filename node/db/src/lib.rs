pub mod errors;
pub mod rocks;

use errors::Error;
use std::path::Path;

pub trait DatabaseService {
    fn start(path: &Path) -> Result<Self, Error>
    where
        Self: Sized;
}

pub trait Write {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>;
    fn bulk_write(&self, keys: &[Vec<u8>], values: &[Vec<u8>]) -> Result<(), Error>;
    fn bulk_delete(&self, keys: &[Vec<u8>]) -> Result<(), Error>;
}

pub trait Read {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>;
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>;
    fn bulk_read(&self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>, Error>;
}
