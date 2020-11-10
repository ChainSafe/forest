// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;
use thiserror::Error;

/// Cid Error
#[derive(Debug, Error)]
pub enum Error {
    #[error("Unknown codec")]
    UnknownCodec,
    #[error("Input too short")]
    InputTooShort,
    #[error("Failed to parse multihash")]
    ParsingError,
    #[error("Unrecognized CID version")]
    InvalidCidVersion,
    #[error(transparent)]
    Multihash(#[from] multihash::Error),
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
