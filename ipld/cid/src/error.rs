// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use multibase;
use multihash;
use std::{error, fmt, io};
/// Error types
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Error {
    UnknownCodec,
    InputTooShort,
    ParsingError,
    InvalidCidVersion,
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::UnknownCodec => write!(f, "Unknown codec"),
            Error::InputTooShort => write!(f, "Input too short"),
            Error::ParsingError => write!(f, "Failed to parse multihash"),
            Error::InvalidCidVersion => write!(f, "Unrecognized CID version"),
            Error::Other(err) => write!(f, "Other cid Error: {}", err.clone()),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        use self::Error::*;

        match self {
            UnknownCodec => "Unknown codec",
            InputTooShort => "Input too short",
            ParsingError => "Failed to parse multihash",
            InvalidCidVersion => "Unrecognized CID version",
            Other(_) => "Other Cid Error",
        }
    }
}

impl From<io::Error> for Error {
    fn from(_: io::Error) -> Error {
        Error::ParsingError
    }
}

impl From<multibase::Error> for Error {
    fn from(_: multibase::Error) -> Error {
        Error::ParsingError
    }
}

impl From<multihash::DecodeOwnedError> for Error {
    fn from(_: multihash::DecodeOwnedError) -> Error {
        Error::ParsingError
    }
}

impl From<multihash::EncodeError> for Error {
    fn from(_: multihash::EncodeError) -> Error {
        Error::ParsingError
    }
}

impl From<multihash::DecodeError> for Error {
    fn from(_: multihash::DecodeError) -> Error {
        Error::ParsingError
    }
}

impl From<Error> for fmt::Error {
    fn from(_: Error) -> fmt::Error {
        fmt::Error {}
    }
}
