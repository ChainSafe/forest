//// Copyright 2020 ChainSafe Systems
//// SPDX-License-Identifier: Apache-2.0, MIT
use std::fmt;
use std::error;
use serde::export::Formatter;

#[derive(Debug)]
pub enum Error {
    ParsingError(String),
    Other(String)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::ParsingError(err) => write!(f, "Failed to parse CAR file: {}", err.clone()),
            Error::Other(err) => write!(f, "Other cid Error: {}", err.clone()),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        use self::Error::*;

        match self {
            ParsingError(_) => "Failed to parse CAR file",
            Other(_) => "Other Cid Error",
        }
    }
}

//
//impl From<io::Error> for Error {
//    fn from(_: io::Error) -> Error {
//        Error::ParsingError
//    }
//}
//
