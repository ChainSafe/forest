// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;
use thiserror::Error;

/// Cid Error
#[derive(PartialEq, Eq, Clone, Debug, Error)]
pub enum Error {
    #[error("Unknown codec")]
    UnknownCodec,
    #[error("Input too short")]
    InputTooShort,
    #[error("Failed to parse multihash")]
    ParsingError,
    #[error("Unrecognized CID version")]
    InvalidCidVersion,
    #[error("Other cid Error: {0}")]
    Other(String),
}

impl From<io::Error> for Error {
    fn from(_err: io::Error) -> Error {
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
