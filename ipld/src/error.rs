// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_encoding::error::*;
use serde::ser;
use std::fmt::{self, Debug};
use thiserror::Error;

/// IPLD error
#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    #[error("{0}")]
    Encoding(String),
    #[error("{0}")]
    Decoding(String),
    #[error("{0}")]
    Other(&'static str),
    #[error("Failed to traverse link: {0}")]
    Link(String),
    #[error("{0}")]
    Custom(String),
}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Error {
        Error::Encoding(msg.to_string())
    }
}

impl<E: Debug> From<CborEncodeError<E>> for Error {
    fn from(e: CborEncodeError<E>) -> Error {
        Error::Encoding(format!("{e:?}"))
    }
}

impl<E: Debug> From<CborDecodeError<E>> for Error {
    fn from(e: CborDecodeError<E>) -> Error {
        Error::Decoding(format!("{e:?}"))
    }
}
