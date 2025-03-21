// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::Error as ChainError;
use fvm_ipld_encoding::Error as EncodeError;
use thiserror::Error;

/// `MessagePool` error.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    /// Error indicating message that's too large
    #[error("Message is too big")]
    MessageTooBig,
    #[error("gas price is lower than min gas price")]
    GasPriceTooLow,
    #[error("gas fee cap is too low")]
    GasFeeCapTooLow,
    #[error("Cannot send more Filecoin than will ever exist")]
    MessageValueTooHigh,
    #[error("Message sequence too low")]
    SequenceTooLow,
    #[error("Not enough funds to execute transaction")]
    NotEnoughFunds,
    #[cfg(test)]
    #[error("Invalid to address for message")]
    InvalidToAddr,
    #[error("Invalid from address")]
    InvalidFromAddr,
    #[error("Message with sequence already in mempool")]
    DuplicateSequence,
    #[error("Validation Error: {0}")]
    SoftValidationFailure(String),
    #[error("Too many pending messages from actor {0} (trusted: {1})")]
    TooManyPendingMessages(String, bool),
    #[error("{0}")]
    Other(String),
}

impl From<ChainError> for Error {
    fn from(ce: ChainError) -> Self {
        Error::Other(ce.to_string())
    }
}

impl From<EncodeError> for Error {
    fn from(ee: EncodeError) -> Self {
        Error::Other(ee.to_string())
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}
