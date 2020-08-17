// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;
use encoding::Error as CborError;

// Payment Channel Errors
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    #[error("Channel not tracked")]
    ChannelNotTracked,
    #[error("Already Tracking Channel")]
    DupChannelTracking,
    #[error("Address not found")]
    NoAddress,
    #[error("No value in PayChannel Store for given key")]
    NoVal,
    #[error("{0}")]
    Encoding(String),
    #[error("{0}")]
    Other(String),
}

impl From<CborError> for Error {
    fn from(e: CborError) -> Error {
        Error::Encoding(e.to_string())
    }
}
