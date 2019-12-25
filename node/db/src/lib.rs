use std::io;
use std::path::Path;

// Note DB's auto close at end of lifetime
// Note key might be vec<u8>
pub trait DatabaseService {
    fn open(path: &Path) -> Result<Self, io::Error>;
}

pub trait Write {
    fn write(&self, key: vec<u8>, value: vec<u8>) -> Result<(), io:Error>;
    fn delete(&self, key: vec<u8>) -> Result<(), io:Error>;
    fn bulk_write(&self, keys: [vec<u8>], values: [vec<u8>]) -> Result<(), io::Error>;
    fn bulk_delete(&self, keys: [vec<u8>]) -> Result<(), io::Error>;
}

pub trait Read {
    fn read(&self, key: vec<u8>) -> Result<vec<u8>, io::Error>;
    fn exists(&self, key: vec<u8>) -> bool;
    fn bulk_read(&self, keys: [vec<u8>]) -> Result<[vec<u8>], io::Error>;
}

#[derive(Debug)]
struct DB {
    path: String
}