use rocksdb;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    RocksDb(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::RocksDb(err) => write!(f, "Unable to open RocksDb database: {}", err),
        }
    }
}

impl From<rocksdb::Error> for Error {
    fn from(e: rocksdb::Error) -> Error {
        Error::RocksDb(String::from(e))
    }
}
