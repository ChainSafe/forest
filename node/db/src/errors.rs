use rocksdb;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    DbError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::DbError(err) => write!(f, "Database specific error: {}", err),
        }
    }
}

impl From<rocksdb::Error> for Error {
    fn from(e: rocksdb::Error) -> Error {
        Error::DbError(String::from(e))
    }
}
