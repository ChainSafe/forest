// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_encoding::error::Error as EncodingError;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum RPCError {
    Codec(String),
    Custom(String),
}

impl From<std::io::Error> for RPCError {
    fn from(err: std::io::Error) -> Self {
        Self::Custom(err.to_string())
    }
}

impl From<EncodingError> for RPCError {
    fn from(err: EncodingError) -> Self {
        Self::Codec(err.to_string())
    }
}

impl fmt::Display for RPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RPCError::Codec(err) => write!(f, "Codec Error: {}", err),
            RPCError::Custom(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for RPCError {
    fn description(&self) -> &str {
        "Libp2p RPC Error"
    }
}
