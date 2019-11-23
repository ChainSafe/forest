use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    EncodingError,
    DecodingError,
    SigningError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::EncodingError => write!(f, "Error in encoding data"),
            Error::DecodingError => write!(f, "Error in decoding data"),
            Error::SigningError(ref s) => write!(f, "Could not sign data: {}", s),
        }
    }
}

impl From<Box<dyn std::error::Error>> for Error {
    fn from(err: Box<dyn std::error::Error>) -> Error {
        // Pass error encountered in signer trait as module error type
        Error::SigningError(err.description().to_string())
    }
}
