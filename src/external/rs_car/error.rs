use std::io;

use ipld_core::cid::{self, multihash, Cid};

#[derive(Debug)]
pub enum CarDecodeError {
    InvalidCarV1Header(String),
    InvalidCarV2Header(String),
    InvalidMultihash(String),
    InvalidCid(String),
    InvalidBlockHeader(String),
    BlockDigestMismatch(String),
    UnsupportedHashCode((HashCode, Cid)),
    BlockStartEOF,
    UnsupportedCarVersion { version: u64 },
    IoError(io::Error),
}

#[derive(Debug)]
pub enum HashCode {
    Code(u64),
}

impl std::fmt::Display for CarDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CarDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CarDecodeError::IoError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for CarDecodeError {
    fn from(error: io::Error) -> Self {
        CarDecodeError::IoError(error)
    }
}

impl From<multihash::Error> for CarDecodeError {
    fn from(error: multihash::Error) -> Self {
        CarDecodeError::InvalidMultihash(format!("{:?}", error))
    }
}

impl From<cid::Error> for CarDecodeError {
    fn from(error: cid::Error) -> Self {
        CarDecodeError::InvalidCid(format!("{:?}", error))
    }
}
