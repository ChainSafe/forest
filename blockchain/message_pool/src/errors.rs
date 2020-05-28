// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

// /// MessagePool error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Error indicating message that's too large
    #[error("Message is too big")]
    MessageTooBig,
    #[error("Cannot send more Filecoin than will ever exist")]
    MessageValueTooHigh,
    #[error("Message nonce too low")]
    NonceTooLow,
    #[error("not enough funds to execute transaction")]
    NotEnoughFunds,
    #[error("invalid to address for message")]
    InvalidToAddr,
    #[error("message with nonce already in mempool")]
    DuplicateNonce,
    #[error("signature validation failed")]
    SigVerification,
    #[error("Unknown signature type")]
    UnknownSigType,
    #[error("BLS signature too short")]
    BLSSigTooShort,
    #[error("{0}")]
    Other(String),
}
