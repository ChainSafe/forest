use std::io;

// Note some DB's may auto close at end of lifetime, should still be handled
// Note key might be vec<u8>
// Note do we need a tokio runtime?
pub trait DatabaseService {
    fn open(&self) -> Result<(), io::Error>;
    fn close(&self) -> Result<(), io::Error>;
}

pub trait Write {
    fn delete(&self, key: vec<u8>) -> Result<(), io:Error>;
    fn bulk_delete(&self, keys: [vec<u8>]) -> Result<(), io::Error>;
    fn put(&self, key: vec<u8>, value: vec<u8>) -> Result<(), io:Error>;
    fn bulk_put(&self, keys: [vec<u8>], values: [vec<u8>]) -> Result<(), io::Error>;
}

pub trait Read {
    fn get(&self, key: vec<u8>) -> Result<vec<u8>, io::Error>;
    fn bulk_get(&self, keys: [vec<u8>]) -> Result<[vec<u8>], io::Error>;
    fn exists(&self, key: vec<u8>) -> bool;
    fn bulk_exists(&self, keys: [vec<u8>]) -> [bool]; // Might not be usefull
}

#[derive(Debug)]
struct DB {
    path: String
}