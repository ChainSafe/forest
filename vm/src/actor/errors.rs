use crate::ExitCode;

use encoding::Error as EncodingError;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    Aborted(ExitCode),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Aborted(ref c) => write!(f, "Critical error, aborted: {:?}", c),
            Error::Other(ref s) => write!(f, "Error in Actor execution: {}", s),
        }
    }
}

impl From<EncodingError> for Error {
    fn from(e: EncodingError) -> Error {
        Error::Other(e.to_string())
    }
}
