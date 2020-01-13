// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use address::Error as AddressError;
use encoding::Error as EncodingError;
use secp256k1::Error as SecpError;
use std::error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Failed to produce a signature
    SigningError(String),
    /// Unable to perform ecrecover with the given params
    InvalidRecovery(String),
    /// Provided public key is not understood
    InvalidPubKey(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SigningError(s) => write!(f, "Could not sign data: {}", s),
            Error::InvalidRecovery(s) => {
                write!(f, "Could not recover public key from signature: {}", s)
            }
            Error::InvalidPubKey(s) => {
                write!(f, "Invalid generated pub key to create address: {}", s)
            }
        }
    }
}

impl From<Box<dyn error::Error>> for Error {
    fn from(err: Box<dyn error::Error>) -> Error {
        // Pass error encountered in signer trait as module error type
        Error::SigningError(err.description().to_string())
    }
}

impl From<AddressError> for Error {
    fn from(err: AddressError) -> Error {
        // convert error from generating address
        Error::InvalidPubKey(err.to_string())
    }
}

impl From<SecpError> for Error {
    fn from(err: SecpError) -> Error {
        match err {
            SecpError::InvalidRecoveryId => Error::InvalidRecovery(format!("{:?}", err)),
            _ => Error::SigningError(format!("{:?}", err)),
        }
    }
}

impl From<EncodingError> for Error {
    fn from(err: EncodingError) -> Error {
        // Pass error encountered in signer trait as module error type
        Error::SigningError(err.to_string())
    }
}
