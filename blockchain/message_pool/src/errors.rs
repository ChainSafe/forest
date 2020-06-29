// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain::Error as ChainError;
use encoding::Error as EncodeError;
use thiserror::Error;

// /// MessagePool error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Error indicating message that's too large
    #[error("Message is too big")]
    MessageTooBig,
    #[error("Cannot send more Filecoin than will ever exist")]
    MessageValueTooHigh,
    #[error("Message sequence too low")]
    SequenceTooLow,
    #[error("Not enough funds to execute transaction")]
    NotEnoughFunds,
    #[error("Invalid to address for message")]
    InvalidToAddr,
    #[error("Invalid from address")]
    InvalidFromAddr,
    #[error("Message with sequence already in mempool")]
    DuplicateSequence,
    #[error("Signature validation failed")]
    SigVerification,
    #[error("Unknown signature type")]
    UnknownSigType,
    #[error("BLS signature too short")]
    BLSSigTooShort,
    #[error("{0}")]
    Other(String),
    #[error("Mutex is either poisoned or data inside could not be accessed at this current time")]
    MutexPoisonError,
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
